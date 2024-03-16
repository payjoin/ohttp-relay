use http::Uri;

/// A normalized gateway origin URI with a default port if none is specified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayUri(Uri);

impl GatewayUri {
    pub fn new(mut gateway_origin: Uri) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (scheme, default_port) = match gateway_origin.scheme_str() {
            Some("http") => ("http", 80),
            Some("https") | None => ("https", 443),
            _ => return Err("Unsupported URI scheme".into()),
        };

        if gateway_origin.authority().map(|a| a.port().is_none()).unwrap_or(true) {
            let authority = if let Some(auth) = gateway_origin.authority() {
                format!("{}:{}", auth.host(), default_port)
            } else {
                return Err("URI must have an authority".into());
            };

            let path_and_query = gateway_origin
                .path_and_query()
                .map(|pq| pq.to_string())
                .unwrap_or_else(|| "/".to_string());

            let builder =
                Uri::builder().scheme(scheme).authority(authority).path_and_query(path_and_query);

            gateway_origin = builder.build()?;
        }

        Ok(Self(gateway_origin))
    }
}

impl std::ops::Deref for GatewayUri {
    type Target = Uri;

    fn deref(&self) -> &Self::Target { &self.0 }
}

impl From<GatewayUri> for Uri {
    fn from(val: GatewayUri) -> Self { val.0 }
}
