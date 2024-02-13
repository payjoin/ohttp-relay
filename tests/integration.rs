mod integration {
    use std::net::SocketAddr;

    use hex::FromHex;
    use http_body_util::{combinators::BoxBody, BodyExt, Full};
    use hyper::{body::{Bytes, Incoming}, header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE}, server::conn::http1, service::service_fn, Request, Response};
    use hyper_rustls::HttpsConnectorBuilder;
    use hyper_util::{client::legacy::Client, rt::{TokioExecutor, TokioIo}};
    use tokio::net::{TcpListener, TcpStream, UnixStream};

    use ohttp_relay::*;

    const ENCAPSULATED_REQ: &str = "010020000100014b28f881333e7c164ffc499ad9796f877f4e1051ee6d31bad19dec96c208b4726374e469135906992e1268c594d2a10c695d858c40a026e7965e7d86b83dd440b2c0185204b4d63525";
    const ENCAPSULATED_RES: &str =
        "c789e7151fcba46158ca84b04464910d86f9013e404feea014e7be4a441f234f857fbd";

    /// See: https://www.ietf.org/rfc/rfc9458.html#name-complete-example-of-a-reque
    #[tokio::test]
    async fn test_request_response() {
        let gateway_port = find_free_port();
        let relay_port = find_free_port();
        tokio::select! {
            _ = example_gateway_http(gateway_port) => {
                assert!(false, "Gateway is long running");
            }
            _ = listen_tcp(relay_port, format!("http://localhost:{}", gateway_port)) => {
                assert!(false, "Relay is long running");
            }
            _ = ohttp_req_over_tcp(relay_port) => {}
        }
    }

    #[tokio::test]
    async fn test_request_response_socket() {
        let temp_dir = std::env::temp_dir();
        let socket_path = temp_dir.as_path().join("test.socket");

        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("Failed to remove existing socket file");
        }

        let gateway_port = find_free_port();
        let socket_path_str = socket_path.to_str().unwrap();
        tokio::select! {
            _ = example_gateway_http(gateway_port) => {
                assert!(false, "Gateway is long running");
            }
            _ = listen_socket(socket_path_str, format!("http://localhost:{}", gateway_port)) => {
                assert!(false, "Relay is long running");
            }
            _ = ohttp_req_over_unix_socket(socket_path_str) => {}
        }
    }

    async fn example_gateway_http(port: u16) -> Result<(), Box<dyn std::error::Error>> {
        example_gateway(port, |stream| {
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                if let Err(err) =
                    http1::Builder::new().serve_connection(io, service_fn(handle_gateway)).await
                {
                    println!("Failed to serve connection: {:?}", err);
                }
            });
        })
        .await
    }

    async fn handle_gateway(
        req: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        let res = match req.uri().path() {
            "/" => handle_ohttp_req(req).await,
            #[cfg(feature = "bootstrap")]
            "/ohttp-keys" => bootstrap::handle_ohttp_keys(req).await,
            _ => panic!("Unexpected request"),
        }
        .unwrap();
        Ok(res)
    }

    async fn handle_ohttp_req(
        _: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        let mut res = Response::new(full(Vec::from_hex(ENCAPSULATED_RES).unwrap()).boxed());
        *res.status_mut() = hyper::StatusCode::OK;
        res.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("message/ohttp-res"));
        res.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("35"));
        Ok(res)
    }

    async fn ohttp_req_over_tcp(relay_port: u16) -> () {
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

    async fn ohttp_req_over_unix_socket(socket_path: &str) -> () {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let stream = TokioIo::new(UnixStream::connect(socket_path).await.unwrap());
        let (mut sender, conn) = hyper::client::conn::http1::handshake(stream).await.unwrap();
        tokio::task::spawn(async move {
            conn.await.unwrap();
        });
        let mut req = Request::new(full(Vec::from_hex(ENCAPSULATED_REQ).unwrap()).boxed());
        *req.method_mut() = hyper::Method::POST;
        *req.uri_mut() = format!("http://unix-socket-ignores-this.com").parse().unwrap();
        req.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("message/ohttp-req"));
        req.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("78"));
        let res = sender.send_request(req).await.unwrap();
        assert_eq!(res.status(), hyper::StatusCode::OK);
        assert_eq!(
            res.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("message/ohttp-res"))
        );
        assert_eq!(res.headers().get(CONTENT_LENGTH), Some(&HeaderValue::from_static("35")));
    }

    async fn example_gateway<F>(port: u16, handle_conn: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(TcpStream) + Clone + Send + Sync + 'static,
    {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr).await?;
        println!("Gateway listening on port {}", port);

        loop {
            let (stream, _) = listener.accept().await?;
            let handle_conn = handle_conn.clone();

            tokio::task::spawn(async move {
                handle_conn(stream);
            });
        }
    }

    fn find_free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    #[cfg(feature = "bootstrap")]
    mod bootstrap {
        use std::io::Write;
        use std::sync::Arc;

        use rustls::pki_types::{self, CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        use rustls::ServerConfig;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_rustls::{TlsAcceptor, TlsConnector};
        use tokio_tungstenite::connect_async;

        use super::*;
        use ohttp_relay::bootstrap::ws::WsIo;

        const OHTTP_KEYS: &str = "01002031e1f05a740102115220e9af918f738674aec95f54db6e04eb705aae8e79815500080001000100010003";

        #[tokio::test]
        async fn test_bootstrap() {
            let gateway_port = find_free_port();
            let relay_port = find_free_port();
            let (key, cert) = gen_localhost_cert();
            let cert_clone = cert.clone();
            tokio::select! {
                _ = example_gateway_https(gateway_port, (key, cert)) => {
                    assert!(false, "Gateway is long running");
                }
                _ = listen_tcp(relay_port, format!("http://localhost:{}", gateway_port)) => {
                    assert!(false, "Relay is long running");
                }
                _ = ohttp_keys_ws_client(relay_port, cert_clone) => {}
            }
        }

        async fn example_gateway_https(
            port: u16,
            cert_pair: (PrivateKeyDer<'static>, CertificateDer<'static>),
        ) -> Result<(), Box<dyn std::error::Error>> {
            let acceptor = Arc::new(build_tls_acceptor(cert_pair));

            example_gateway(port, move |stream| {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let stream = acceptor.accept(stream).await.expect("TLS error");
                    let io = TokioIo::new(stream);
                    if let Err(err) =
                        http1::Builder::new().serve_connection(io, service_fn(handle_gateway)).await
                    {
                        println!("Failed to serve connection: {:?}", err);
                    }
                });
            })
            .await
        }

        pub(crate) async fn handle_ohttp_keys(
            _: Request<Incoming>,
        ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
            let mut res = Response::new(full(Vec::from_hex(OHTTP_KEYS).unwrap()).boxed());
            *res.status_mut() = hyper::StatusCode::OK;
            res.headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("application/ohttp-keys"));
            res.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("45"));
            Ok(res)
        }

        async fn ohttp_keys_ws_client(relay_port: u16, cert: CertificateDer<'_>) {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let mut root_store = rustls::RootCertStore::empty();
            root_store.add(cert).unwrap();
            let config = tokio_rustls::rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();

            let (ws_stream, _res) =
                connect_async(format!("ws://localhost:{}/ohttp-keys", relay_port))
                    .await
                    .expect("Failed to connect");
            println!("Connected to ws");
            let ws_io = WsIo::new(ws_stream);
            let connector = TlsConnector::from(Arc::new(config));
            let domain = pki_types::ServerName::try_from("localhost")
                .map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid dnsname")
                })
                .unwrap()
                .to_owned();
            let mut tls_stream = connector.connect(domain, ws_io).await.unwrap();

            let content =
                b"GET /ohttp-keys HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            tls_stream.write_all(content).await.unwrap();
            tls_stream.flush().await.unwrap();
            let mut plaintext = Vec::new();
            let _ = tls_stream.read_to_end(&mut plaintext).await.unwrap();
            std::io::stdout().write_all(&plaintext).unwrap();
        }

        fn build_tls_acceptor(
            cert_pair: (PrivateKeyDer<'static>, CertificateDer<'static>),
        ) -> TlsAcceptor {
            let server_config = ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert_pair.1], cert_pair.0)
                .unwrap();
            tokio_rustls::TlsAcceptor::from(Arc::new(server_config))
        }

        fn gen_localhost_cert() -> (PrivateKeyDer<'static>, CertificateDer<'static>) {
            let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
            let key =
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.serialize_private_key_der()));
            let cert = CertificateDer::from(cert.serialize_der().unwrap());
            (key, cert)
        }
    }

    fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
        Full::new(chunk.into()).map_err(|never| match never {}).boxed()
    }
}