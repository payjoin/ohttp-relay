use std::net::SocketAddr;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, HOST};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use once_cell::sync::Lazy;
use tokio::net::{TcpListener, TcpStream};

const PAYJO_IN: &str = "payjo.in";
static OHTTP_RELAY_HOST: Lazy<HeaderValue> =
    Lazy::new(|| HeaderValue::from_str("localhost").expect("Invalid HeaderValue"));
static EXPECTED_MEDIA_TYPE: Lazy<HeaderValue> =
    Lazy::new(|| HeaderValue::from_str("message/ohttp-req").expect("Invalid HeaderValue"));

async fn ohttp_relay(
    mut req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    if req.method() != hyper::Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(full("Method Not Allowed"))
            .unwrap());
    }
    let content_type_header = req.headers().get(CONTENT_TYPE).cloned();
    let content_length_header = req.headers().get(CONTENT_LENGTH).cloned();
    req.headers_mut().clear();
    req.headers_mut().insert(HOST, OHTTP_RELAY_HOST.to_owned());
    if content_type_header != Some(EXPECTED_MEDIA_TYPE.to_owned()) {
        return Ok(Response::builder()
            .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            .body(full("Unsupported Media Type"))
            .unwrap());
    }
    if let Some(content_length) = content_length_header {
        req.headers_mut().insert(CONTENT_LENGTH, content_length);
    }

    let uri_string = format!(
        "http://{}{}",
        PAYJO_IN,
        req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("/")
    );
    let uri = uri_string.parse().unwrap();
    println!("uri: {:?}", uri);
    *req.uri_mut() = uri;

    let host = req.uri().host().expect("uri has no host");
    let port = req.uri().port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);

    async move {
        let client_stream = TcpStream::connect(addr).await.unwrap();
        let io = TokioIo::new(client_stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        sender.send_request(req).await
    }
    .await
    .map(|b| Response::new(b.boxed()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) =
                http1::Builder::new().serve_connection(io, service_fn(ohttp_relay)).await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}
