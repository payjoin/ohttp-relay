use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};

use crate::{empty, full};

pub(crate) enum Error {
    BadGateway,
    MethodNotAllowed,
    UnsupportedMediaType,
    BadRequest(String),
}

impl Error {
    pub fn to_response(&self) -> Response<BoxBody<Bytes, hyper::Error>> {
        let mut res = Response::new(empty());
        match self {
            Self::UnsupportedMediaType => *res.status_mut() = StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::BadGateway => *res.status_mut() = StatusCode::BAD_GATEWAY,
            Self::MethodNotAllowed => *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED,
            Self::BadRequest(e) => {
                *res.status_mut() = StatusCode::BAD_REQUEST;
                *res.body_mut() = full(e.clone()).boxed();
            }
        };

        res
    }
}
