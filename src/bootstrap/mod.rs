use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};

use crate::error::Error;

#[cfg(feature = "connect-bootstrap")]
pub mod connect;

#[cfg(feature = "ws-bootstrap")]
pub mod ws;

pub(crate) async fn handle_ohttp_keys(
    mut req: Request<Incoming>,
    gateway_origin: Arc<String>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Error> {
    #[cfg(feature = "connect-bootstrap")]
    if connect::is_connect_request(&req) {
        return connect::try_upgrade(req, gateway_origin.clone())
            .await
            .map_err(|e| Error::BadRequest(format!("Error upgrading to CONNECT: {}", e)));
    }

    #[cfg(feature = "ws-bootstrap")]
    if ws::is_websocket_request(&req) {
        let res = ws::upgrade(&mut req, gateway_origin)
            .await
            .map_err(|e| Error::BadRequest(format!("Error upgrading to websocket: {}", e)))?;
        let (parts, body) = res.into_parts();
        let boxbody = body.map_err(|never| match never {}).boxed();
        return Ok(Response::from_parts(parts, boxbody));
    }

    Err(Error::BadRequest("Not a supported proxy upgrade request".to_string()))
}
