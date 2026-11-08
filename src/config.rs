use std::net::SocketAddr;
use std::path::PathBuf;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub public_base_url: String,
    pub admin_token: Option<String>,
    pub allow_non_loopback: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let bind: SocketAddr = std::env::var("BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .map_err(|err| AppError::bad(format!("BIND is not a socket address: {err}")))?;
        let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into()));
        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .unwrap_or_else(|_| format!("http://127.0.0.1:{}", bind.port()));
        let admin_token = std::env::var("ADMIN_TOKEN").ok().filter(|s| !s.is_empty());
        let allow_non_loopback = matches!(
            std::env::var("ALLOW_NON_LOOPBACK").ok().as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes")
        );
        Ok(Self {
            bind,
            data_dir,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
            admin_token,
            allow_non_loopback,
        })
    }

    pub fn ensure_bind_allowed(&self) -> Result<(), AppError> {
        if self.bind.ip().is_loopback() || self.allow_non_loopback {
            Ok(())
        } else {
            Err(AppError::bad(
                "refusing non-loopback BIND; set ALLOW_NON_LOOPBACK=1 when Docker publishes the port",
            ))
        }
    }
}

pub fn admin_authorized(expected: Option<&str>, presented: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let Some(presented) = presented else {
        return false;
    };
    constant_time_eq(expected.as_bytes(), presented.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in a.iter().zip(b.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

pub fn bearer_from_header(value: Option<&str>) -> Option<&str> {
    value
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_when_admin_token_unset() {
        assert!(admin_authorized(None, None));
        assert!(admin_authorized(None, Some("anything")));
    }

    #[test]
    fn admin_token_must_match() {
        assert!(admin_authorized(Some("s3cret"), Some("s3cret")));
        assert!(!admin_authorized(Some("s3cret"), Some("other")));
        assert!(!admin_authorized(Some("s3cret"), None));
    }
}
