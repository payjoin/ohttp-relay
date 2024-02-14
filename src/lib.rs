use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, HOST};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use once_cell::sync::Lazy;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, UnixListener};
use tokio_util::net::Listener;

pub mod error;
use crate::error::Error;

#[cfg(any(feature = "connect-bootstrap", feature = "ws-bootstrap"))]
pub mod bootstrap;

pub const DEFAULT_PORT: u16 = 3000;
pub static OHTTP_RELAY_HOST: Lazy<HeaderValue> =
    Lazy::new(|| HeaderValue::from_str("localhost").expect("Invalid HeaderValue"));
pub static EXPECTED_MEDIA_TYPE: Lazy<HeaderValue> =
    Lazy::new(|| HeaderValue::from_str("message/ohttp-req").expect("Invalid HeaderValue"));

pub async fn listen_tcp(
    port: u16,
    gateway_origin: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    println!("OHTTP relay listening on tcp://{}", addr);
    ohttp_relay(listener, gateway_origin).await
}

pub async fn listen_socket(
    socket_path: &str,
    gateway_origin: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = UnixListener::bind(socket_path)?;
    println!("OHTTP relay listening on socket: {}", socket_path);
    ohttp_relay(listener, gateway_origin).await
}

async fn ohttp_relay<L>(
    mut listener: L,
    gateway_origin: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    L: Listener + Unpin,
    L::Io: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let gateway_origin = Arc::new(gateway_origin);

    while let Ok((stream, _)) = listener.accept().await {
        let gateway_origin = gateway_origin.clone();
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| serve_ohttp_relay(req, gateway_origin.clone())),
                )
                .with_upgrades()
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }

    Ok(())
}

async fn serve_ohttp_relay(
    req: Request<Incoming>,
    gateway_origin: Arc<String>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    println!("req: {:?}", req);
    let res = match req.method() {
        &Method::POST => handle_ohttp_relay(req, gateway_origin.as_str()).await,
        #[cfg(any(feature = "connect-bootstrap", feature = "ws-bootstrap"))]
        &Method::CONNECT | &Method::GET =>
            crate::bootstrap::handle_ohttp_keys(req, gateway_origin).await,
        _ => Err(Error::NotFound),
    }
    .unwrap_or_else(|e| e.to_response());
    Ok(res)
}

async fn handle_ohttp_relay(
    req: Request<Incoming>,
    gateway_origin: &str,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    let fwd_req = into_forward_req(req, gateway_origin)?;
    forward_request(fwd_req).await.map(|res| {
        let (parts, body) = res.into_parts();
        let boxed_body = BoxBody::new(body);
        Response::from_parts(parts, boxed_body)
    })
}

/// Convert an incoming request into a request to forward to the target gateway server.
fn into_forward_req(
    mut req: Request<Incoming>,
    gateway_origin: &str,
) -> Result<Request<Incoming>, Error> {
    if req.method() != hyper::Method::POST {
        return Err(Error::MethodNotAllowed);
    }
    let content_type_header = req.headers().get(CONTENT_TYPE).cloned();
    let content_length_header = req.headers().get(CONTENT_LENGTH).cloned();
    req.headers_mut().clear();
    req.headers_mut().insert(HOST, OHTTP_RELAY_HOST.to_owned());
    if content_type_header != Some(EXPECTED_MEDIA_TYPE.to_owned()) {
        return Err(Error::UnsupportedMediaType);
    }
    if let Some(content_length) = content_length_header {
        req.headers_mut().insert(CONTENT_LENGTH, content_length);
    }

    let uri_string = format!(
        "{}{}",
        gateway_origin,
        req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("/")
    );
    let uri = uri_string.parse().map_err(|_| Error::BadRequest("Invalid target uri".to_owned()))?;
    *req.uri_mut() = uri;
    Ok(req)
}

async fn forward_request(req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
    let https =
        HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
    let client = Client::builder(TokioExecutor::new()).build(https);
    client.request(req).await.map_err(|_| Error::BadGateway)
}

pub(crate) fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new().map_err(|never| match never {}).boxed()
}

pub(crate) fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}
