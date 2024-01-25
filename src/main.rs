use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, HOST};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use once_cell::sync::Lazy;
use tokio::net::TcpListener;

mod error;
use crate::error::Error;

const DEFAULT_PORT: u16 = 3000;
static OHTTP_RELAY_HOST: Lazy<HeaderValue> =
    Lazy::new(|| HeaderValue::from_str("localhost").expect("Invalid HeaderValue"));
static EXPECTED_MEDIA_TYPE: Lazy<HeaderValue> =
    Lazy::new(|| HeaderValue::from_str("message/ohttp-req").expect("Invalid HeaderValue"));

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port: u16 =
        std::env::var("PORT").map(|s| s.parse().expect("Invalid PORT")).unwrap_or(DEFAULT_PORT);
    let gateway_origin = std::env::var("GATEWAY_ORIGIN").expect("GATEWAY_ORIGIN is required");
    ohttp_relay(port, gateway_origin).await
}

async fn ohttp_relay(
    port: u16,
    gateway_origin: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = TcpListener::bind(addr).await?;
    println!("OHTTP relay listening on http://{}", addr);
    let gateway_origin = Arc::new(gateway_origin);
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let gateway_origin = gateway_origin.clone();
        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| serve_ohttp_relay(req, gateway_origin.clone())),
                )
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn serve_ohttp_relay(
    req: Request<Incoming>,
    gateway_origin: Arc<String>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let res =
        handle_ohttp_relay(req, gateway_origin.as_str()).await.unwrap_or_else(|e| e.to_response());
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
        let res = Response::from_parts(parts, boxed_body);
        res
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
    println!("uri: {:?}", uri);
    *req.uri_mut() = uri;
    Ok(req)
}

async fn forward_request(req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
    let https =
        HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
    let client = Client::builder(TokioExecutor::new()).build(https);
    client.request(req).await.map_err(|_| Error::BadGateway)
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new().map_err(|never| match never {}).boxed()
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}

#[cfg(test)]
mod test {
    use hex::FromHex;

    use super::*;

    const ENCAPSULATED_REQ: &str = "010020000100014b28f881333e7c164ffc499ad9796f877f4e1051ee6d31bad19dec96c208b4726374e469135906992e1268c594d2a10c695d858c40a026e7965e7d86b83dd440b2c0185204b4d63525";
    const ENCAPSULATED_RES: &str =
        "c789e7151fcba46158ca84b04464910d86f9013e404feea014e7be4a441f234f857fbd";

    /// See: https://www.ietf.org/rfc/rfc9458.html#name-complete-example-of-a-reque
    #[tokio::test]
    async fn test_request_response() {
        let gateway_port = find_free_port();
        let relay_port = find_free_port();
        tokio::select! {
            _ = ohttp_gateway(gateway_port) => {
                assert!(false, "Gateway is long running");
            }
            _ = ohttp_relay(relay_port, format!("http://localhost:{}", gateway_port)) => {
                assert!(false, "Relay is long running");
            }
            _ = ohttp_client(relay_port) => {}
        }
    }

    async fn ohttp_gateway(port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        let listener = TcpListener::bind(addr).await?;
        println!("Gateway listening on http://{}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);

            tokio::task::spawn(async move {
                if let Err(err) =
                    http1::Builder::new().serve_connection(io, service_fn(handle_gateway)).await
                {
                    println!("Failed to serve connection: {:?}", err);
                }
            });
        }
    }

    async fn handle_gateway(
        _: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        let mut res = Response::new(full(Vec::from_hex(ENCAPSULATED_RES).unwrap()).boxed());
        *res.status_mut() = hyper::StatusCode::OK;
        res.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("message/ohttp-res"));
        res.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("35"));
        Ok(res)
    }

    async fn ohttp_client(relay_port: u16) -> () {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let mut req = Request::new(full(Vec::from_hex(ENCAPSULATED_REQ).unwrap()).boxed());
        *req.method_mut() = hyper::Method::POST;
        *req.uri_mut() = format!("http://127.0.0.1:{}/", relay_port).parse().unwrap();
        req.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("message/ohttp-req"));
        req.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("78"));
        let https =
            HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
        let client = Client::builder(TokioExecutor::new()).build(https);
        let res = client.request(req).await.unwrap();
        assert_eq!(res.status(), hyper::StatusCode::OK);
        assert_eq!(
            res.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("message/ohttp-res"))
        );
        assert_eq!(res.headers().get(CONTENT_LENGTH), Some(&HeaderValue::from_static("35")));
    }

    fn find_free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }
}
