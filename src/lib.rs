use std::net::SocketAddr;
use std::sync::Arc;

pub use gateway_uri::GatewayUri;
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
use hyper_rustls::builderstates::WantsSchemes;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, UnixListener};
use tokio_util::net::Listener;
use tracing::{error, info, instrument};

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
    gateway_origin: GatewayUri,
) -> Result<tokio::task::JoinHandle<Result<(), BoxError>>, BoxError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    println!("OHTTP relay listening on tcp://{}", addr);
    ohttp_relay(listener, RelayConfig::new_with_default_client(gateway_origin)).await
}

#[instrument]
pub async fn listen_socket(
    socket_path: &str,
    gateway_origin: GatewayUri,
) -> Result<tokio::task::JoinHandle<Result<(), BoxError>>, BoxError> {
    let listener = UnixListener::bind(socket_path)?;
    info!("OHTTP relay listening on socket: {}", socket_path);
    ohttp_relay(listener, RelayConfig::new_with_default_client(gateway_origin)).await
}

#[cfg(feature = "_test-util")]
pub async fn listen_tcp_on_free_port(
    default_gateway: GatewayUri,
    root_store: rustls::RootCertStore,
) -> Result<(u16, tokio::task::JoinHandle<Result<(), BoxError>>), BoxError> {
    let listener = tokio::net::TcpListener::bind("[::]:0").await?;
    let port = listener.local_addr()?.port();
    println!("OHTTP relay binding to port {}", listener.local_addr()?);
    let config = RelayConfig::new(default_gateway, root_store);
    let handle = ohttp_relay(listener, config).await?;
    Ok((port, handle))
}

#[derive(Debug)]
struct RelayConfig {
    default_gateway: GatewayUri,
    client: HttpClient,
}

impl RelayConfig {
    fn new_with_default_client(default_gateway: GatewayUri) -> Self {
        Self::new(default_gateway, HttpClient::default())
    }

    fn new(default_gateway: GatewayUri, into_client: impl Into<HttpClient>) -> Self {
        RelayConfig { default_gateway, client: into_client.into() }
    }
}

#[derive(Debug, Clone)]
struct HttpClient(hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, Incoming>);

impl std::ops::Deref for HttpClient {
    type Target = hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, Incoming>;
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl From<HttpsConnectorBuilder<WantsSchemes>> for HttpClient {
    fn from(builder: HttpsConnectorBuilder<WantsSchemes>) -> Self {
        let https = builder.https_or_http().enable_http1().build();
        Self(Client::builder(TokioExecutor::new()).build(https))
    }
}

impl Default for HttpClient {
    fn default() -> Self { HttpsConnectorBuilder::new().with_webpki_roots().into() }
}

impl From<rustls::RootCertStore> for HttpClient {
    fn from(root_store: rustls::RootCertStore) -> Self {
        HttpsConnectorBuilder::new()
            .with_tls_config(
                rustls::ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth(),
            )
            .into()
    }
}

#[instrument(skip(listener))]
async fn ohttp_relay<L>(
    mut listener: L,
    config: RelayConfig,
) -> Result<tokio::task::JoinHandle<Result<(), BoxError>>, BoxError>
where
    L: Listener + Unpin + Send + 'static,
    L::Io: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let config = Arc::new(config);

    let handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let config = config.clone();
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service_fn(|req| serve_ohttp_relay(req, &config)))
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
    config: &RelayConfig,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let path = req.uri().path();
    let mut res = match (req.method(), path) {
        (&Method::OPTIONS, _) => Ok(handle_preflight()),
        (&Method::GET, "/health") => Ok(health_check().await),
        (&Method::POST, "/") => handle_ohttp_relay(req, config).await,
        #[cfg(any(feature = "connect-bootstrap", feature = "ws-bootstrap"))]
        (&Method::CONNECT, _) | (&Method::GET, _) =>
            crate::bootstrap::handle_ohttp_keys(req, &config.default_gateway).await,
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
    config: &RelayConfig,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    let fwd_req = into_forward_req(req, &config.default_gateway)?;
    forward_request(fwd_req, config).await.map(|res| {
        let (parts, body) = res.into_parts();
        let boxed_body = BoxBody::new(body);
        Response::from_parts(parts, boxed_body)
    })
}

/// Convert an incoming request into a request to forward to the target gateway server.
#[instrument]
fn into_forward_req(
    mut req: Request<Incoming>,
    gateway_origin: &GatewayUri,
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

    *req.uri_mut() = gateway_origin.to_uri();
    Ok(req)
}

#[instrument]
async fn forward_request(
    req: Request<Incoming>,
    config: &RelayConfig,
) -> Result<Response<Incoming>, Error> {
    config.client.request(req).await.map_err(|_| Error::BadGateway)
}

pub(crate) fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new().map_err(|never| match never {}).boxed()
}

pub(crate) fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}
