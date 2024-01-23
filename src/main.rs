use std::net::SocketAddr;

use hyper::body::Incoming;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};

const PAYJO_IN: &str = "payjo.in";
const OHTTP_RELAY_HOST: &str = "localhost";

async fn ohttp_relay(
    mut req: Request<hyper::body::Incoming>,
) -> Result<Response<Incoming>, hyper::Error> {
    let content_type_header = req.headers().get(CONTENT_TYPE).cloned();
    let content_length_header = req.headers().get(CONTENT_LENGTH).cloned();
    req.headers_mut().clear();
    req.headers_mut().insert(HOST, OHTTP_RELAY_HOST.parse().unwrap());
    if let Some(content_type) = content_type_header {
        req.headers_mut().insert(CONTENT_TYPE, content_type);
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
