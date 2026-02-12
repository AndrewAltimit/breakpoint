use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Authentication configuration loaded from environment variables.
#[derive(Clone)]
pub struct AuthConfig {
    /// Bearer token for REST API access. None = auth disabled.
    pub bearer_token: Option<String>,
    /// GitHub webhook HMAC secret. None = signature verification disabled.
    /// Used by the webhook handler (webhooks module).
    pub github_webhook_secret: Option<String>,
}

/// Axum middleware that validates Bearer token authentication.
/// If no token is configured (`AuthConfig::bearer_token` is None), all
/// requests are allowed through (auth disabled).
pub async fn bearer_auth_middleware(
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_config = request
        .extensions()
        .get::<AuthConfig>()
        .cloned()
        .unwrap_or(AuthConfig {
            bearer_token: None,
            github_webhook_secret: None,
        });

    if let Some(ref expected) = auth_config.bearer_token {
        let provided = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match provided {
            Some(token) if token == expected => {},
            _ => return Err(StatusCode::UNAUTHORIZED),
        }
    }

    Ok(next.run(request).await)
}

/// Verify a GitHub webhook HMAC-SHA256 signature.
/// `signature` is the `X-Hub-Signature-256` header value (e.g. "sha256=abcdef...").
/// `secret` is the shared webhook secret.
/// `body` is the raw request body bytes.
pub fn verify_github_signature(signature: &str, secret: &str, body: &[u8]) -> bool {
    type HmacSha256 = Hmac<Sha256>;

    let Some(hex_sig) = signature.strip_prefix("sha256=") else {
        return false;
    };

    let Ok(expected_bytes) = hex::decode(hex_sig) else {
        return false;
    };

    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };

    mac.update(body);
    mac.verify_slice(&expected_bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_valid_signature() {
        let secret = "test-secret";
        let body = b"hello world";

        // Compute expected signature
        let mut mac = <Hmac<Sha256>>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let result = mac.finalize().into_bytes();
        let sig = format!("sha256={}", hex::encode(result));

        assert!(verify_github_signature(&sig, secret, body));
    }

    #[test]
    fn verify_invalid_signature() {
        assert!(!verify_github_signature(
            "sha256=0000000000000000000000000000000000000000000000000000000000000000",
            "test-secret",
            b"hello world"
        ));
    }

    #[test]
    fn verify_malformed_signature() {
        assert!(!verify_github_signature("invalid", "secret", b"body"));
        assert!(!verify_github_signature(
            "sha256=notvalidhex!",
            "secret",
            b"body"
        ));
    }
}
