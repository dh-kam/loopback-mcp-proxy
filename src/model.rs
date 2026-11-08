use chrono_lite::unix_now;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Transport {
    #[serde(rename = "streamable-http")]
    StreamableHttp,
    #[serde(rename = "sse")]
    Sse,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StreamableHttp => "streamable-http",
            Self::Sse => "sse",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "streamable-http" => Ok(Self::StreamableHttp),
            "sse" => Ok(Self::Sse),
            other => Err(AppError::internal(format!("unknown transport {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthKind {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "bearer")]
    Bearer,
    #[serde(rename = "basic")]
    Basic,
    #[serde(rename = "headers")]
    Headers,
    #[serde(rename = "oauth2")]
    OAuth2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OAuthGrant {
    #[serde(rename = "authorization_code")]
    AuthorizationCode,
    #[serde(rename = "client_credentials")]
    ClientCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    pub grant: OAuthGrant,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub authorize_url: Option<String>,
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub kind: AuthKind,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub basic_username: Option<String>,
    #[serde(default)]
    pub basic_password: Option<String>,
    #[serde(default)]
    pub basic_realm: Option<String>,
    #[serde(default)]
    pub headers: Vec<HeaderPair>,
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
}

impl AuthConfig {
    pub fn none() -> Self {
        Self {
            kind: AuthKind::None,
            bearer_token: None,
            basic_username: None,
            basic_password: None,
            basic_realm: None,
            headers: Vec::new(),
            oauth: None,
        }
    }

    pub fn public_view(&self) -> AuthPublic {
        AuthPublic {
            kind: self.kind,
            bearer_configured: self
                .bearer_token
                .as_ref()
                .is_some_and(|token| !token.is_empty()),
            basic_username: self.basic_username.clone(),
            basic_realm: self.basic_realm.clone(),
            header_names: self.headers.iter().map(|h| h.name.clone()).collect(),
            oauth: self.oauth.as_ref().map(|oauth| OAuthPublic {
                grant: oauth.grant,
                client_id: oauth.client_id.clone(),
                authorize_url: oauth.authorize_url.clone(),
                token_url: oauth.token_url.clone(),
                scopes: oauth.scopes.clone(),
                has_client_secret: oauth.client_secret.as_ref().is_some_and(|s| !s.is_empty()),
                has_access_token: oauth.access_token.as_ref().is_some_and(|s| !s.is_empty()),
                has_refresh_token: oauth.refresh_token.as_ref().is_some_and(|s| !s.is_empty()),
                expires_at: oauth.expires_at,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ToolPolicyMode {
    #[default]
    AllowAll,
    ReadOnly,
    Allowlist,
    Denylist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolPolicy {
    #[serde(default)]
    pub mode: ToolPolicyMode,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl ToolPolicy {
    pub fn is_tool_allowed(&self, name: &str, tool_def: Option<&Value>) -> bool {
        let clean = name.trim();
        if self.matches_list(&self.denied_tools, clean) {
            return false;
        }
        match self.mode {
            ToolPolicyMode::AllowAll | ToolPolicyMode::Denylist => true,
            ToolPolicyMode::Allowlist => self.matches_list(&self.allowed_tools, clean),
            ToolPolicyMode::ReadOnly => {
                self.matches_list(&self.allowed_tools, clean)
                    || Self::classify_tool_risk(clean, tool_def) == "read"
            }
        }
    }

    pub fn filter_tools(&self, tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .filter(|tool| {
                let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
                    return false;
                };
                self.is_tool_allowed(name, Some(tool))
            })
            .cloned()
            .collect()
    }

    pub fn classify_tool_risk(name: &str, tool_def: Option<&Value>) -> &'static str {
        if let Some(annotations) = tool_def.and_then(|t| t.get("annotations")) {
            if annotations
                .get("destructiveHint")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return "destructive";
            }
            if let Some(ro) = annotations.get("readOnlyHint").and_then(|v| v.as_bool()) {
                if ro {
                    return "read";
                }
            }
        }

        let lower = name.to_ascii_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        const DESTRUCTIVE_VERBS: &[&str] = &[
            "delete",
            "remove",
            "drop",
            "destroy",
            "purge",
            "truncate",
            "revoke",
            "wipe",
            "kill",
            "terminate",
            "refund",
            "ban",
            "unpublish",
            "erase",
            "reset",
        ];
        const WRITE_VERBS: &[&str] = &[
            "create",
            "update",
            "upsert",
            "insert",
            "add",
            "set",
            "write",
            "put",
            "post",
            "patch",
            "edit",
            "modify",
            "send",
            "publish",
            "merge",
            "close",
            "reopen",
            "assign",
            "move",
            "rename",
            "copy",
            "duplicate",
            "execute",
            "exec",
            "run",
            "trigger",
            "start",
            "stop",
            "restart",
            "deploy",
            "apply",
            "commit",
            "push",
            "transfer",
            "approve",
            "reject",
            "comment",
            "reply",
            "upload",
            "import",
            "archive",
            "cancel",
            "clear",
        ];
        const READ_VERBS: &[&str] = &[
            "get",
            "list",
            "search",
            "read",
            "fetch",
            "find",
            "query",
            "inspect",
            "describe",
            "show",
            "view",
            "check",
            "verify",
            "count",
            "ping",
            "whoami",
            "me",
            "status",
            "info",
            "lookup",
            "resolve",
            "retrieve",
            "browse",
            "export",
            "download",
        ];

        if tokens.iter().any(|t| DESTRUCTIVE_VERBS.contains(t)) {
            return "destructive";
        }
        if tokens.iter().any(|t| WRITE_VERBS.contains(t)) {
            return "write";
        }
        if tokens.iter().any(|t| READ_VERBS.contains(t)) {
            return "read";
        }
        if let Some(annotations) = tool_def.and_then(|t| t.get("annotations")) {
            if annotations.get("readOnlyHint").and_then(|v| v.as_bool()) == Some(false) {
                return "write";
            }
        }
        "read"
    }

    fn matches_list(&self, list: &[String], name: &str) -> bool {
        list.iter().any(|pattern| {
            let p = pattern.trim();
            if p.is_empty() {
                return false;
            }
            if p == "*" {
                return true;
            }
            if let Some(prefix) = p.strip_suffix('*') {
                name.starts_with(prefix)
            } else {
                p == name
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub transport: Transport,
    pub enabled: bool,
    pub auth: AuthConfig,
    #[serde(default)]
    pub tool_policy: ToolPolicy,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    pub name: String,
    pub slug: String,
    pub url: String,
    pub transport: Transport,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub auth: AuthConfig,
    #[serde(default)]
    pub tool_policy: Option<ToolPolicy>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnState {
    Idle,
    Disabled,
    Connecting,
    Connected,
    AuthRequired,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthPublic {
    pub kind: AuthKind,
    pub bearer_configured: bool,
    pub basic_username: Option<String>,
    pub basic_realm: Option<String>,
    pub header_names: Vec<String>,
    pub oauth: Option<OAuthPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthPublic {
    pub grant: OAuthGrant,
    pub client_id: String,
    pub authorize_url: Option<String>,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub has_client_secret: bool,
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub transport: Transport,
    pub enabled: bool,
    pub auth: AuthPublic,
    pub tool_policy: ToolPolicy,
    pub state: ConnState,
    pub message: String,
    pub tool_count: usize,
    pub exposed_tool_count: usize,
    pub tools: Vec<Value>,
    pub server_info: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    pub checked_at: Option<String>,
    #[serde(default)]
    pub total_calls: u64,
    #[serde(default)]
    pub error_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub seq: u64,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub slug: String,
    pub source: String,
    pub kind: String,
    pub phase: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BusEvent {
    Upsert { server: ServerView },
    Remove { id: String },
    Activity { entry: ActivityEntry },
    Ping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub auth_required: bool,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub registration_endpoint: Option<String>,
    pub scopes: Vec<String>,
    pub realm: Option<String>,
    pub resource: Option<String>,
    pub message: String,
}

pub fn same_origin(a: &str, b: &str) -> bool {
    let (Ok(left), Ok(right)) = (url::Url::parse(a.trim()), url::Url::parse(b.trim())) else {
        return false;
    };
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

pub fn validate_target_url(
    raw_url: &str,
    self_port: Option<u16>,
    public_base_url: Option<&str>,
) -> Result<url::Url, AppError> {
    let url =
        url::Url::parse(raw_url.trim()).map_err(|err| AppError::bad(format!("url: {err}")))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(AppError::bad(format!(
                "url scheme {other} is not supported"
            )))
        }
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::bad("url must include a host"))?
        .to_ascii_lowercase();
    let clean_host = host.trim_start_matches('[').trim_end_matches(']');

    // Cloud metadata & link-local SSRF block
    if matches!(
        clean_host,
        "169.254.169.254"
            | "metadata.google.internal"
            | "metadata"
            | "100.100.100.200"
            | "fd00:ec2::254"
    ) {
        return Err(AppError::bad(
            "upstream URL cannot target cloud metadata endpoints (SSRF blocked)",
        ));
    }
    if let Ok(ip) = clean_host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) if v4.is_link_local() => {
                return Err(AppError::bad(
                    "upstream URL cannot target link-local IPv4 addresses (SSRF blocked)",
                ));
            }
            std::net::IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80 => {
                return Err(AppError::bad(
                    "upstream URL cannot target link-local IPv6 addresses (SSRF blocked)",
                ));
            }
            _ => {}
        }
    }

    // Loopback self-recursion guard
    if let Some(pub_url) = public_base_url {
        if same_origin(raw_url, pub_url) {
            return Err(AppError::bad(
                "upstream URL cannot point to this proxy's own public base URL (self-recursion blocked)",
            ));
        }
    }
    if let Some(port) = self_port {
        if url.port_or_known_default() == Some(port) {
            let is_local = matches!(clean_host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
                || clean_host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified());
            if is_local {
                return Err(AppError::bad(
                    "upstream URL cannot point to this proxy's own loopback listener (self-recursion blocked)",
                ));
            }
        }
    }

    Ok(url)
}

pub fn validate_server(input: &ServerInput, previous: Option<&Server>) -> Result<(), AppError> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::bad("name must be 1..=80 characters"));
    }
    if !valid_slug(&input.slug) {
        return Err(AppError::bad(
            "slug must be 1..=48 chars of lowercase letters, digits, and hyphens",
        ));
    }
    validate_target_url(&input.url, None, None)?;
    let reusable_auth = previous
        .filter(|server| same_origin(&server.url, &input.url))
        .map(|server| &server.auth);
    validate_auth(&input.auth, reusable_auth)?;
    Ok(())
}

fn valid_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    if bytes.is_empty() || bytes.len() > 48 {
        return false;
    }
    let mut prev_hyphen = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        let ok = byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
        if !ok {
            return false;
        }
        if byte == b'-' && (index == 0 || index + 1 == bytes.len() || prev_hyphen) {
            return false;
        }
        prev_hyphen = byte == b'-';
    }
    true
}

fn validate_auth(auth: &AuthConfig, previous: Option<&AuthConfig>) -> Result<(), AppError> {
    for header in &auth.headers {
        if !valid_header_name(&header.name) {
            return Err(AppError::bad(format!(
                "invalid header name {}",
                header.name
            )));
        }
        if header
            .value
            .bytes()
            .any(|b| b == b'\r' || b == b'\n' || b == 0)
        {
            return Err(AppError::bad(format!(
                "header {} contains an illegal character",
                header.name
            )));
        }
    }
    match auth.kind {
        AuthKind::None => Ok(()),
        AuthKind::Bearer => {
            if secret_present(&auth.bearer_token)
                || (previous.is_some_and(|prev| prev.kind == AuthKind::Bearer)
                    && secret_present(&previous.and_then(|prev| prev.bearer_token.clone())))
            {
                Ok(())
            } else {
                Err(AppError::bad("bearer token is required"))
            }
        }
        AuthKind::Basic => {
            let user = auth.basic_username.as_deref().unwrap_or("").trim();
            if user.is_empty() {
                return Err(AppError::bad("basic username is required"));
            }
            let password_ok = secret_present(&auth.basic_password)
                || previous.is_some_and(|prev| {
                    prev.kind == AuthKind::Basic && secret_present(&prev.basic_password)
                });
            if password_ok {
                Ok(())
            } else {
                Err(AppError::bad("basic password is required"))
            }
        }
        AuthKind::Headers => {
            if auth.headers.is_empty() {
                return Err(AppError::bad("at least one header is required"));
            }
            let all_have_values = auth.headers.iter().all(|h| {
                !h.value.is_empty()
                    || previous.is_some_and(|prev| {
                        prev.headers.iter().any(|old| {
                            old.name.eq_ignore_ascii_case(&h.name) && !old.value.is_empty()
                        })
                    })
            });
            if all_have_values {
                Ok(())
            } else {
                Err(AppError::bad("header values are required"))
            }
        }
        AuthKind::OAuth2 => {
            let Some(oauth) = auth.oauth.as_ref() else {
                return Err(AppError::bad("oauth settings are required"));
            };
            if oauth.grant == OAuthGrant::ClientCredentials && oauth.token_url.trim().is_empty() {
                return Err(AppError::bad("oauth token url is required"));
            }
            if oauth.grant == OAuthGrant::ClientCredentials && oauth.client_id.trim().is_empty() {
                return Err(AppError::bad("oauth client id is required"));
            }
            Ok(())
        }
    }
}

fn secret_present(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|s| !s.is_empty())
}

pub fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

pub fn merge_server_auth(
    previous: Option<&Server>,
    incoming_url: &str,
    incoming: AuthConfig,
) -> AuthConfig {
    let reusable = previous
        .filter(|server| same_origin(&server.url, incoming_url))
        .map(|server| &server.auth);
    merge_auth(reusable, incoming)
}

pub fn merge_auth(previous: Option<&AuthConfig>, mut incoming: AuthConfig) -> AuthConfig {
    let Some(prev) = previous else {
        return incoming;
    };
    if prev.kind != incoming.kind {
        return incoming;
    }
    incoming.bearer_token = keep_secret(incoming.bearer_token, prev.bearer_token.clone());
    incoming.basic_password = keep_secret(incoming.basic_password, prev.basic_password.clone());
    incoming.headers = merge_headers(&prev.headers, &incoming.headers);
    if let (Some(next), Some(old)) = (incoming.oauth.as_mut(), prev.oauth.as_ref()) {
        next.client_secret = keep_secret(next.client_secret.clone(), old.client_secret.clone());
        let replaced_access = next
            .access_token
            .as_ref()
            .is_some_and(|token| !token.is_empty());
        if replaced_access {
            if next.expires_at.is_none() {
                next.expires_at = None;
            }
        } else {
            next.access_token = old.access_token.clone();
            next.expires_at = old.expires_at;
        }
        next.refresh_token = keep_secret(next.refresh_token.clone(), old.refresh_token.clone());
        if next.client_id.is_empty() {
            next.client_id = old.client_id.clone();
        }
        if next.token_url.is_empty() {
            next.token_url = old.token_url.clone();
        }
        if next.authorize_url.is_none() {
            next.authorize_url = old.authorize_url.clone();
        }
        if next.registration_endpoint.is_none() {
            next.registration_endpoint = old.registration_endpoint.clone();
        }
        if next.resource.is_none() {
            next.resource = old.resource.clone();
        }
    }
    incoming
}

fn keep_secret(incoming: Option<String>, previous: Option<String>) -> Option<String> {
    match incoming {
        Some(value) if !value.is_empty() => Some(value),
        _ => previous,
    }
}

fn merge_headers(previous: &[HeaderPair], incoming: &[HeaderPair]) -> Vec<HeaderPair> {
    incoming
        .iter()
        .map(|header| {
            if header.value.is_empty() {
                if let Some(old) = previous
                    .iter()
                    .find(|candidate| candidate.name.eq_ignore_ascii_case(&header.name))
                {
                    return old.clone();
                }
            }
            header.clone()
        })
        .collect()
}

pub fn split_namespaced<'a>(full: &'a str, slugs: &[String]) -> Option<(&'a str, &'a str)> {
    let (slug, tool) = full.split_once("__")?;
    if tool.is_empty() || !slugs.iter().any(|candidate| candidate == slug) {
        None
    } else {
        Some((slug, tool))
    }
}

pub fn namespaced_tool(slug: &str, tool: &Value) -> Value {
    let upstream = tool
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("tool")
        .to_string();
    let description = tool
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let prefixed = if description.is_empty() {
        format!("[{slug}]")
    } else {
        format!("[{slug}] {description}")
    };
    let mut owned = tool.clone();
    if let Some(object) = owned.as_object_mut() {
        object.insert("name".into(), Value::String(format!("{slug}__{upstream}")));
        object.insert("description".into(), Value::String(prefixed));
    }
    owned
}

pub fn apply_token_response(oauth: &mut OAuthConfig, body: &Value) {
    if let Some(token) = body.get("access_token").and_then(|v| v.as_str()) {
        oauth.access_token = Some(token.to_string());
    }
    if let Some(token) = body.get("refresh_token").and_then(|v| v.as_str()) {
        if !token.is_empty() {
            oauth.refresh_token = Some(token.to_string());
        }
    }
    if let Some(expires_in) = body.get("expires_in").and_then(|v| v.as_i64()) {
        oauth.expires_at = Some(unix_now().saturating_add(expires_in));
    }
}

pub fn token_needs_refresh(expires_at: Option<i64>, now: i64) -> bool {
    match expires_at {
        Some(exp) => now.saturating_add(60) >= exp,
        None => false,
    }
}

pub fn redact_and_truncate_json(value: &Value, max_chars: usize) -> String {
    let redacted = redact_value(value);
    let raw = serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".into());
    truncate_one_line(&raw, max_chars)
}

pub fn summarize_tool_result(value: &Value, max_chars: usize) -> String {
    if let Some(text) = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|first| first.get("text"))
        .and_then(Value::as_str)
    {
        return truncate_one_line(text, max_chars);
    }
    redact_and_truncate_json(value, max_chars)
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut next = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                if lower.contains("token")
                    || lower.contains("secret")
                    || lower.contains("password")
                    || lower.contains("authorization")
                    || lower.contains("api_key")
                    || lower.contains("apikey")
                    || lower.contains("credential")
                {
                    next.insert(k.clone(), Value::String("***".into()));
                } else {
                    next.insert(k.clone(), redact_value(v));
                }
            }
            Value::Object(next)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        other => other.clone(),
    }
}

fn truncate_one_line(text: &str, max_chars: usize) -> String {
    let clean = text.replace(['\n', '\r'], " ");
    let mut chars = clean.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

pub mod chrono_lite {
    pub fn unix_now() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    pub fn rfc3339_now() -> String {
        format_rfc3339(unix_now())
    }

    pub fn format_rfc3339(unix: i64) -> String {
        if unix < 0 {
            return unix.to_string();
        }
        let days = unix.div_euclid(86_400);
        let tod = unix.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let hour = tod / 3600;
        let min = (tod % 3600) / 60;
        let sec = tod % 60;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
    }

    fn civil_from_days(days: i64) -> (i32, u32, u32) {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = (z - era * 146_097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        (y as i32, m as u32, d as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bearer(token: &str) -> AuthConfig {
        AuthConfig {
            kind: AuthKind::Bearer,
            bearer_token: Some(token.into()),
            basic_username: None,
            basic_password: None,
            basic_realm: None,
            headers: Vec::new(),
            oauth: None,
        }
    }

    #[test]
    fn slug_rules() {
        assert!(valid_slug("github"));
        assert!(valid_slug("a1"));
        assert!(!valid_slug("Git"));
        assert!(!valid_slug("-a"));
        assert!(!valid_slug("a__b"));
        assert!(!valid_slug("a--b"));
    }

    #[test]
    fn namespaced_split_keeps_tool_underscores() {
        let slugs = vec!["github".to_string()];
        let (slug, tool) = split_namespaced("github__search__code", &slugs).unwrap();
        assert_eq!(slug, "github");
        assert_eq!(tool, "search__code");
        assert!(split_namespaced("other__search", &slugs).is_none());
    }

    #[test]
    fn merge_keeps_blank_secrets() {
        let previous = bearer("stored");
        let mut incoming = bearer("");
        incoming.bearer_token = None;
        let merged = merge_auth(Some(&previous), incoming);
        assert_eq!(merged.bearer_token.as_deref(), Some("stored"));
    }

    #[test]
    fn origin_change_prevents_secret_reuse() {
        let server = Server {
            id: "1".into(),
            name: "GitHub".into(),
            slug: "github".into(),
            url: "https://api.githubcopilot.com/mcp".into(),
            transport: Transport::StreamableHttp,
            enabled: true,
            auth: bearer("stored-secret"),
            tool_policy: ToolPolicy::default(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let mut same_host_input = ServerInput {
            name: "GitHub".into(),
            slug: "github".into(),
            url: "https://api.githubcopilot.com/v2/mcp".into(),
            transport: Transport::StreamableHttp,
            enabled: true,
            auth: bearer(""),
            tool_policy: None,
        };
        same_host_input.auth.bearer_token = None;
        assert!(validate_server(&same_host_input, Some(&server)).is_ok());
        let merged_same = merge_server_auth(
            Some(&server),
            &same_host_input.url,
            same_host_input.auth.clone(),
        );
        assert_eq!(merged_same.bearer_token.as_deref(), Some("stored-secret"));

        let mut evil_host_input = same_host_input.clone();
        evil_host_input.url = "https://attacker.example.com/mcp".into();
        assert!(validate_server(&evil_host_input, Some(&server)).is_err());
        let merged_evil =
            merge_server_auth(Some(&server), &evil_host_input.url, evil_host_input.auth);
        assert!(merged_evil.bearer_token.is_none());
    }

    #[test]
    fn refresh_window_is_sixty_seconds() {
        assert!(token_needs_refresh(Some(100), 40));
        assert!(token_needs_refresh(Some(100), 50));
        assert!(!token_needs_refresh(Some(100), 30));
        assert!(!token_needs_refresh(None, 10_000));
    }

    #[test]
    fn rfc3339_epoch() {
        assert_eq!(chrono_lite::format_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn tool_policy_read_only_and_denylist_rules() {
        assert_eq!(ToolPolicy::classify_tool_risk("search_pages", None), "read");
        assert_eq!(ToolPolicy::classify_tool_risk("create_issue", None), "write");
        assert_eq!(
            ToolPolicy::classify_tool_risk("delete_database", None),
            "destructive"
        );

        let ro_policy = ToolPolicy {
            mode: ToolPolicyMode::ReadOnly,
            allowed_tools: vec!["create_comment".into()],
            denied_tools: vec!["search_secret*".into()],
        };
        assert!(ro_policy.is_tool_allowed("search_pages", None));
        assert!(!ro_policy.is_tool_allowed("search_secrets", None));
        assert!(!ro_policy.is_tool_allowed("delete_page", None));
        assert!(!ro_policy.is_tool_allowed("update_page", None));
        assert!(ro_policy.is_tool_allowed("create_comment", None));
    }
}
