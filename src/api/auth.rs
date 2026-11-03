//! Authentication and anti-CSRF checks for The Oracle API.
//!
//! The API used to accept any request from any origin. Because it is bound to
//! loopback and browsers happily send cross-origin requests to `localhost`, any
//! web page the user visited could read the config (which includes provider API
//! keys) and launch or stop workers.
//!
//! Two independent controls are applied to every `/api/v1` route:
//! * a bearer token, generated at start-up when no `DREAMSWARM_API_TOKEN` is set;
//! * an `Origin`/`Host` check that rejects DNS-rebinding and cross-site callers.

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

pub const TOKEN_ENV_VAR: &str = "DREAMSWARM_API_TOKEN";
pub const TOKEN_HEADER: &str = "x-dreamswarm-token";
pub const TELEMETRY_STREAM_PATH: &str = "/api/v1/telemetry/stream";

#[derive(Clone)]
pub struct ApiSecurity {
    token_digest: Arc<[u8; 32]>,
    allowed_origins: Arc<HashSet<String>>,
    allowed_hosts: Arc<HashSet<String>>,
}

impl std::fmt::Debug for ApiSecurity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render the digest or token material in logs.
        f.debug_struct("ApiSecurity").finish_non_exhaustive()
    }
}

impl ApiSecurity {
    /// Builds security state for `bind_host:port`.
    ///
    /// The token is read from `DREAMSWARM_API_TOKEN`; when absent a fresh random
    /// one is generated and returned alongside the state so the caller can show
    /// it to the local operator.
    pub fn bootstrap(bind_host: &str, port: u16) -> anyhow::Result<(Self, Option<String>)> {
        let (token, generated) = match std::env::var(TOKEN_ENV_VAR) {
            Ok(value) if !value.trim().is_empty() => (value.trim().to_string(), None),
            _ => {
                let token = generate_token()?;
                (token.clone(), Some(token))
            }
        };

        let mut allowed_hosts: HashSet<String> = HashSet::new();
        let mut allowed_origins: HashSet<String> = HashSet::new();
        for host in [bind_host, "localhost", "127.0.0.1", "[::1]"] {
            add_host_variants(&mut allowed_hosts, host, port);
            for scheme in ["http", "https"] {
                allowed_origins.insert(format!("{}://{}:{}", scheme, host, port));
            }
        }

        Ok((
            Self {
                token_digest: Arc::new(digest(&token)),
                allowed_origins: Arc::new(allowed_origins),
                allowed_hosts: Arc::new(allowed_hosts),
            },
            generated,
        ))
    }

    /// Builds explicit state (used by tests and embedders).
    pub fn new(token: &str, allowed_origins: &[String], allowed_hosts: &[String]) -> Self {
        Self {
            token_digest: Arc::new(digest(token)),
            allowed_origins: Arc::new(allowed_origins.iter().cloned().collect()),
            allowed_hosts: Arc::new(allowed_hosts.iter().cloned().collect()),
        }
    }

    /// Authorizes an SSE stream request.
    ///
    /// `EventSource` cannot attach custom headers, so the token may arrive as
    /// the `token` query parameter. The route and method are checked explicitly
    /// so the query-token path cannot be reused for other endpoints.
    pub fn authorize_stream(
        &self,
        method: &Method,
        uri: &axum::http::Uri,
        headers: &HeaderMap,
    ) -> Result<(), Box<Response>> {
        if !self.host_allowed(headers) || !self.origin_allowed(headers) {
            return Err(Box::new(
                (
                    StatusCode::FORBIDDEN,
                    "Forbidden: request origin is not allowed",
                )
                    .into_response(),
            ));
        }
        if uri.path() != TELEMETRY_STREAM_PATH || *method != Method::GET {
            return Err(Box::new(
                (
                    StatusCode::FORBIDDEN,
                    "Forbidden: query-token authentication is limited to the telemetry stream",
                )
                    .into_response(),
            ));
        }
        if self.token_matches(headers) || self.query_token_matches(uri) {
            Ok(())
        } else {
            Err(Box::new(
                (
                    StatusCode::UNAUTHORIZED,
                    "Unauthorized: missing or invalid API token",
                )
                    .into_response(),
            ))
        }
    }

    fn query_token_matches(&self, uri: &axum::http::Uri) -> bool {
        match uri.query() {
            Some(query) => query.split('&').any(|pair| {
                pair.strip_prefix("token=")
                    .map(|value| constant_time_eq(&digest(value), &self.token_digest))
                    .unwrap_or(false)
            }),
            None => false,
        }
    }

    pub fn authorize(&self, method: &Method, headers: &HeaderMap) -> Result<(), Box<Response>> {
        if !self.host_allowed(headers) {
            return Err(Box::new(
                (
                    StatusCode::FORBIDDEN,
                    "Forbidden: Host header is not allowed",
                )
                    .into_response(),
            ));
        }
        if !self.origin_allowed(headers) {
            return Err(Box::new(
                (
                    StatusCode::FORBIDDEN,
                    "Forbidden: cross-origin request blocked",
                )
                    .into_response(),
            ));
        }
        if !self.token_matches(headers) {
            return Err(Box::new(
                (
                    StatusCode::UNAUTHORIZED,
                    "Unauthorized: missing or invalid API token",
                )
                    .into_response(),
            ));
        }
        if !is_safe_method(method) && !self.has_custom_header(headers) {
            return Err(Box::new(
                (
                    StatusCode::FORBIDDEN,
                    "Forbidden: state-changing request must send the token header",
                )
                    .into_response(),
            ));
        }
        Ok(())
    }

    fn host_allowed(&self, headers: &HeaderMap) -> bool {
        // A missing Host is only possible for HTTP/1.0/a malformed request;
        // reject it rather than treating it as trusted.
        match headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
            Some(host) => self.allowed_hosts.contains(&host.to_ascii_lowercase()),
            None => false,
        }
    }

    fn origin_allowed(&self, headers: &HeaderMap) -> bool {
        match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
            None => true, // same-origin navigations and non-browser clients
            Some(origin) => self.allowed_origins.contains(&origin.to_ascii_lowercase()),
        }
    }

    /// Returns true when the request requires a custom header.
    ///
    /// `Content-Type: application/json` alone would be enough in practice, but
    /// requiring the token header puts mutating calls out of reach of
    /// `<form>`-based CSRF entirely.
    fn has_custom_header(&self, headers: &HeaderMap) -> bool {
        headers.contains_key(TOKEN_HEADER)
    }

    fn token_matches(&self, headers: &HeaderMap) -> bool {
        match headers.get(TOKEN_HEADER).and_then(|v| v.to_str().ok()) {
            Some(candidate) => constant_time_eq(&digest(candidate), &self.token_digest),
            None => false,
        }
    }
}

/// Axum middleware enforcing [`ApiSecurity`].
pub async fn require_auth(
    axum::extract::Extension(security): axum::extract::Extension<ApiSecurity>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let is_stream = request.uri().path() == TELEMETRY_STREAM_PATH
        && request.method() == Method::GET
        && request.uri().query().is_some();
    let outcome = if is_stream {
        security.authorize_stream(request.method(), request.uri(), request.headers())
    } else {
        security.authorize(request.method(), request.headers())
    };
    match outcome {
        Ok(()) => next.run(request).await,
        Err(response) => *response,
    }
}

fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn add_host_variants(hosts: &mut HashSet<String>, host: &str, port: u16) {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    hosts.insert(format!("{}:{}", host, port).to_ascii_lowercase());
    hosts.insert(format!("{}:{}", bare, port).to_ascii_lowercase());
    // GCP/E2B style preview proxies forward the public host in `Host`.
    if let Ok(extra) = std::env::var("DREAMSWARM_API_HOSTS") {
        for value in extra.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            hosts.insert(value.to_ascii_lowercase());
        }
    }
}

fn generate_token() -> anyhow::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| anyhow::anyhow!("could not read system randomness: {}", e))?;
    Ok(bytes.iter().map(|b| format!("{:02x}", b)).collect())
}

fn digest(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"dreamswarm-api-token:");
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (key, value) in pairs {
            map.insert(*key, value.parse().unwrap());
        }
        map
    }

    fn security() -> ApiSecurity {
        ApiSecurity::new(
            "test-token",
            &["http://127.0.0.1:8080".to_string()],
            &["127.0.0.1:8080".to_string(), "localhost:8080".to_string()],
        )
    }

    #[test]
    fn accepts_same_origin_get_with_token() {
        let map = headers(&[
            ("host", "127.0.0.1:8080"),
            ("origin", "http://127.0.0.1:8080"),
            (TOKEN_HEADER, "test-token"),
        ]);
        assert!(security().authorize(&Method::GET, &map).is_ok());
    }

    #[test]
    fn rejects_missing_or_wrong_token() {
        let map = headers(&[("host", "127.0.0.1:8080")]);
        assert_eq!(
            security()
                .authorize(&Method::GET, &map)
                .unwrap_err()
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let map = headers(&[("host", "127.0.0.1:8080"), (TOKEN_HEADER, "wrong")]);
        assert_eq!(
            security()
                .authorize(&Method::GET, &map)
                .unwrap_err()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn rejects_hostile_origin() {
        let map = headers(&[
            ("host", "127.0.0.1:8080"),
            ("origin", "https://evil.example"),
            (TOKEN_HEADER, "test-token"),
        ]);
        assert_eq!(
            security()
                .authorize(&Method::GET, &map)
                .unwrap_err()
                .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn rejects_dns_rebinding_host() {
        let map = headers(&[("host", "evil.example:8080"), (TOKEN_HEADER, "test-token")]);
        assert_eq!(
            security()
                .authorize(&Method::GET, &map)
                .unwrap_err()
                .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn rejects_missing_host() {
        let map = headers(&[(TOKEN_HEADER, "test-token")]);
        assert!(security().authorize(&Method::GET, &map).is_err());
    }

    #[test]
    fn mutating_request_requires_token_header() {
        // Even a same-origin POST without the token header is refused.
        let map = headers(&[("host", "127.0.0.1:8080")]);
        let response = security().authorize(&Method::POST, &map).unwrap_err();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn stream_token_must_be_scoped_to_the_stream_route() {
        let security = security();
        let headers = headers(&[("host", "127.0.0.1:8080")]);
        let stream: axum::http::Uri = "/api/v1/telemetry/stream?token=test-token".parse().unwrap();
        assert!(security
            .authorize_stream(&Method::GET, &stream, &headers)
            .is_ok());

        let other: axum::http::Uri = "/api/v1/config?token=test-token".parse().unwrap();
        assert!(security
            .authorize_stream(&Method::GET, &other, &headers)
            .is_err());

        let bad: axum::http::Uri = "/api/v1/telemetry/stream?token=nope".parse().unwrap();
        assert_eq!(
            security
                .authorize_stream(&Method::GET, &bad, &headers)
                .unwrap_err()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn generated_tokens_are_unique_and_long() {
        let a = generate_token().unwrap();
        let b = generate_token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn constant_time_comparison_detects_difference() {
        assert!(constant_time_eq(&[1u8; 32], &[1u8; 32]));
        let mut other = [1u8; 32];
        other[31] = 2;
        assert!(!constant_time_eq(&[1u8; 32], &other));
    }
}
