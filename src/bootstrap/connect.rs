use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tracing::{debug, error, instrument};

use crate::error::Error;
use crate::{empty, GatewayUri};

pub(crate) fn is_connect_request(req: &Request<Incoming>) -> bool {
    Method::CONNECT == req.method()
}

#[instrument]
pub(crate) async fn try_upgrade(
    req: Request<Incoming>,
    gateway_origin: Arc<GatewayUri>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    if let Some(addr) = find_allowable_gateway(&req, &gateway_origin).await {
        tokio::task::spawn(async move {
            match hyper::upgrade::on(req).await {
                Ok(upgraded) => {
                    if let Err(e) = tunnel(upgraded, addr).await {
                        error!("server io error: {}", e);
                    };
                }
                Err(e) => error!("upgrade error: {}", e),
            }
        });
        Ok(Response::new(empty()))
    } else {
        error!("CONNECT host is not socket addr: {:?}", req.uri());
        Err(Error::BadRequest("CONNECT Host must be a known gateway socket address".to_string()))
    }
}

/// Create a TCP connection to host:port, build a tunnel between the connection and
/// the upgraded connection
#[instrument]
async fn tunnel(upgraded: Upgraded, addr: SocketAddr) -> std::io::Result<()> {
    let mut server = TcpStream::connect(addr).await?;
    let mut upgraded = TokioIo::new(upgraded);
    let (_, _) = tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;
    Ok(())
}

/// Only allow CONNECT requests to the configured OHTTP gateway authority.
/// This prevents the relay from being used as an arbitrary proxy
/// to any host on the internet.
#[instrument]
async fn find_allowable_gateway<B>(
    req: &Request<B>,
    gateway_origin: &GatewayUri,
) -> Option<SocketAddr>
where
    B: Debug,
{
    debug!("req: {:?}, gateway_origin: {:?}", req, gateway_origin);
    if req.uri().authority() != Some(gateway_origin.authority()) {
        debug!("CONNECT request to non-gateway authority: {:?}", req.uri());
        return None;
    }

    gateway_origin.to_socket_addr().await
}

#[cfg(test)]
mod test {
    use once_cell::sync::{Lazy, OnceCell};
    use tracing_subscriber::{self, EnvFilter, FmtSubscriber};

    use super::*;

    static GATEWAY_ORIGIN: Lazy<GatewayUri> =
        Lazy::new(|| GatewayUri::from_static("https://0.0.0.0"));
    static INIT: OnceCell<()> = OnceCell::new();

    #[tokio::test]
    async fn mismatched_gateways_not_allowed() {
        init_tracing();
        let not_gateway_origin = "https://0.0.0.0:4433";
        let req = hyper::Request::builder().uri(not_gateway_origin).body(()).unwrap();
        let allowable_gateway = find_allowable_gateway(&req, &*GATEWAY_ORIGIN);
        assert!(allowable_gateway.await.is_none());
    }

    #[tokio::test]
    async fn matched_gateways_allowed() {
        init_tracing();
        // ensure GatewayUri port is defined automatically
        let req = Request::builder().uri("https://0.0.0.0:443").body(()).unwrap();
        assert!(find_allowable_gateway(&req, &*GATEWAY_ORIGIN).await.is_some());
    }

    fn init_tracing() {
        INIT.get_or_init(|| {
            let subscriber = FmtSubscriber::builder()
                .with_env_filter(EnvFilter::from_default_env())
                .with_test_writer()
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to set global default subscriber");
        });
    }
}
