use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use gateway_uri::GatewayUri;
use http::Uri;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    HeaderValue, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_LENGTH, CONTENT_TYPE, HOST,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, UnixListener};
use tokio_util::net::Listener;
use tracing::{debug, error, info, instrument};

pub mod error;
mod gateway_uri;
use crate::error::{BoxError, Error};

#[cfg(any(feature = "connect-bootstrap", feature = "ws-bootstrap"))]
pub mod bootstrap;

pub const DEFAULT_PORT: u16 = 3000;
pub const OHTTP_RELAY_HOST: HeaderValue = HeaderValue::from_static("0.0.0.0");
pub const EXPECTED_MEDIA_TYPE: HeaderValue = HeaderValue::from_static("message/ohttp-req");

#[instrument]
pub async fn listen_tcp(
    port: u16,
    gateway_origin: Uri,
) -> Result<tokio::task::JoinHandle<Result<(), BoxError>>, BoxError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    println!("OHTTP relay listening on tcp://{}", addr);
    ohttp_relay(listener, gateway_origin).await
}

#[instrument]
pub async fn listen_socket(
    socket_path: &str,
    gateway_origin: Uri,
) -> Result<tokio::task::JoinHandle<Result<(), BoxError>>, BoxError> {
    let listener = UnixListener::bind(socket_path)?;
    info!("OHTTP relay listening on socket: {}", socket_path);
    ohttp_relay(listener, gateway_origin).await
}

#[cfg(feature = "_test-util")]
pub async fn listen_tcp_on_free_port(
    gateway_origin: Uri,
) -> Result<(u16, tokio::task::JoinHandle<Result<(), BoxError>>), BoxError> {
    let listener = tokio::net::TcpListener::bind("[::]:0").await?;
    let port = listener.local_addr()?.port();
    println!("Directory server binding to port {}", listener.local_addr()?);
    let handle = ohttp_relay(listener, gateway_origin).await?;
    Ok((port, handle))
}

#[instrument(skip(listener))]
async fn ohttp_relay<L>(
    mut listener: L,
    gateway_origin: Uri,
) -> Result<tokio::task::JoinHandle<Result<(), BoxError>>, BoxError>
where
    L: Listener + Unpin + Send + 'static,
    L::Io: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let gateway_origin = GatewayUri::new(gateway_origin)?;
    let gateway_origin: Arc<GatewayUri> = Arc::new(gateway_origin);

    let handle = tokio::spawn(async move {
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
                    error!("Error serving connection: {:?}", err);
                }
            });
        }
        Ok(())
    });

    Ok(handle)
}

#[instrument]
async fn serve_ohttp_relay(
    req: Request<Incoming>,
    gateway_origin: Arc<GatewayUri>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let path = req.uri().path();
    let mut res = match (req.method(), path) {
        (&Method::OPTIONS, _) => Ok(handle_preflight()),
        (&Method::GET, "/health") => Ok(health_check().await),
        (&Method::POST, "/") => handle_ohttp_relay(req, &gateway_origin).await,
        #[cfg(any(feature = "connect-bootstrap", feature = "ws-bootstrap"))]
        (&Method::CONNECT, _) | (&Method::GET, _) =>
            crate::bootstrap::handle_ohttp_keys(req, gateway_origin).await,
        _ => Err(Error::NotFound),
    }
    .unwrap_or_else(|e| e.to_response());
    res.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    Ok(res)
}

fn handle_preflight() -> Response<BoxBody<Bytes, hyper::Error>> {
    let mut res = Response::new(empty());
    *res.status_mut() = hyper::StatusCode::NO_CONTENT;
    res.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    res.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("CONNECT, GET, OPTIONS, POST"),
    );
    res.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Content-Length"),
    );
    res
}

async fn health_check() -> Response<BoxBody<Bytes, hyper::Error>> { Response::new(empty()) }

#[instrument]
async fn handle_ohttp_relay(
    req: Request<Incoming>,
    gateway_origin: &GatewayUri,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    let fwd_req = into_forward_req(req, gateway_origin)?;
    forward_request(fwd_req).await.map(|res| {
        let (parts, body) = res.into_parts();
        let boxed_body = BoxBody::new(body);
        Response::from_parts(parts, boxed_body)
    })
}

/// Convert an incoming request into a request to forward to the target gateway server.
#[instrument]
fn into_forward_req(
    mut req: Request<Incoming>,
    gateway_origin: &Uri,
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

    if let Some(path) = req.uri().path_and_query() {
        if path != "/" {
            return Err(Error::NotFound);
        }
    }

    *req.uri_mut() = Uri::builder()
        .scheme(gateway_origin.scheme_str().unwrap_or("https"))
        .authority(
            gateway_origin.authority().expect("Gateway origin must have an authority").as_str(),
        )
        .path_and_query("/")
        .build()
        .map_err(|_| Error::BadRequest("Invalid gateway uri".to_owned()))?;
    Ok(req)
}

#[instrument]
async fn forward_request(req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
    let https =
        HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
    let client = Client::builder(TokioExecutor::new()).build(https);
    client.request(req).await.map_err(|_| Error::BadGateway)
}

#[instrument]
pub(crate) fn uri_to_addr(uri: &Uri) -> Option<SocketAddr> {
    let authority = uri.authority()?;

    let host = authority.host();
    let port = authority.port_u16().or_else(|| {
        match uri.scheme_str() {
            Some("https") => Some(443),
            _ => Some(80), // Default to 80 if it's not https or if the scheme is not specified
        }
    })?;
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    debug!("Resolved address: {:?}", addr);
    Some(addr)
}

pub(crate) fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new().map_err(|never| match never {}).boxed()
}

pub(crate) fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}
