use ohttp_relay::DEFAULT_PORT;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port_env = std::env::var("PORT");
    let unix_socket_env = std::env::var("UNIX_SOCKET");
    let gateway_origin = std::env::var("GATEWAY_ORIGIN").expect("GATEWAY_ORIGIN is required");

    match (port_env, unix_socket_env) {
        (Ok(_), Ok(_)) => panic!(
            "Both PORT and UNIX_SOCKET environment variables are set. Please specify only one."
        ),
        (Err(_), Ok(unix_socket_path)) =>
            ohttp_relay::listen_socket(&unix_socket_path, gateway_origin).await?,
        (Ok(port_str), Err(_)) => {
            let port: u16 = port_str.parse().expect("Invalid PORT");
            ohttp_relay::listen_tcp(port, gateway_origin).await?
        }
        (Err(_), Err(_)) => ohttp_relay::listen_tcp(DEFAULT_PORT, gateway_origin).await?,
    }

    Ok(())
}
