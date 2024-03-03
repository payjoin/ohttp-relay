use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::error::Error;
use crate::{empty, uri_to_addr, GatewayUri};

pub(crate) fn is_connect_request(req: &Request<Incoming>) -> bool {
    Method::CONNECT == req.method()
}

pub(crate) async fn try_upgrade(
    req: Request<Incoming>,
    gateway_origin: Arc<GatewayUri>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    if let Some(addr) = find_allowable_gateway(&req, &gateway_origin) {
        tokio::task::spawn(async move {
            match hyper::upgrade::on(req).await {
                Ok(upgraded) => {
                    if let Err(e) = tunnel(upgraded, addr).await {
                        eprintln!("server io error: {}", e);
                    };
                }
                Err(e) => eprintln!("upgrade error: {}", e),
            }
        });
        Ok(Response::new(empty()))
    } else {
        eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
        Err(Error::BadRequest("CONNECT Host must be a known gateway socket address".to_string()))
    }
}

/// Create a TCP connection to host:port, build a tunnel between the connection and
/// the upgraded connection
async fn tunnel(upgraded: Upgraded, addr: SocketAddr) -> std::io::Result<()> {
    let mut server = TcpStream::connect(addr).await?;
    let mut upgraded = TokioIo::new(upgraded);
    let (_, _) = tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;
    Ok(())
}

/// Only allow CONNECT requests to the configured OHTTP gateway authority.
/// This prevents the relay from being used as an arbitrary proxy
/// to any host on the internet.
fn find_allowable_gateway<B>(req: &Request<B>, gateway_origin: &GatewayUri) -> Option<SocketAddr> {
    if req.uri().authority() != gateway_origin.authority() {
        return None;
    }

    uri_to_addr(gateway_origin)
}

#[cfg(test)]
mod test {
    use http::Uri;
    use hyper::Request;
    use once_cell::sync::Lazy;

    use super::*;

    static GATEWAY_ORIGIN: Lazy<GatewayUri> =
        Lazy::new(|| GatewayUri::new(Uri::from_static("https://0.0.0.0")).unwrap());

    #[test]
    fn mismatched_gateways_not_allowed() {
        let not_gateway_origin = "https://0.0.0.0:4433";
        let req = hyper::Request::builder().uri(not_gateway_origin).body(()).unwrap();
        let allowable_gateway = find_allowable_gateway(&req, &*GATEWAY_ORIGIN);
        assert!(allowable_gateway.is_none());
    }

    #[test]
    fn matched_gateways_allowed() {
        // ensure GatewayUri port is defined automatically
        let req = Request::builder().uri("https://0.0.0.0:443").body(()).unwrap();
        assert!(find_allowable_gateway(&req, &*GATEWAY_ORIGIN).is_some());
    }
}
