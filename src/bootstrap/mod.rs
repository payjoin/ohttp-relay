use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};

use crate::error::Error;

pub mod ws;

pub(crate) async fn handle_ohttp_keys(
    mut req: Request<Incoming>,
    gateway_origin: Arc<String>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    if hyper_tungstenite::is_upgrade_request(&req) {
        let (res, websocket) = hyper_tungstenite::upgrade(&mut req, None)
            .map_err(|e| Error::BadRequest(format!("Error upgrading to websocket: {}", e)))?;
        tokio::spawn(async move {
            if let Err(e) = ws::serve_websocket(websocket, gateway_origin.as_str()).await {
                eprintln!("Error in websocket connection: {e}");
            }
        });
        let (parts, body) = res.into_parts();
        let boxbody = body.map_err(|never| match never {}).boxed();
        Ok(Response::from_parts(parts, boxbody))
    } else {
        Err(Error::BadRequest("Not a websocket upgrade request".to_string()))
    }
}
