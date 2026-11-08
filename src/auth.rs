use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::AppError;
use crate::model::{
    apply_token_response, token_needs_refresh, AuthConfig, AuthKind, Discovery, OAuthConfig,
    OAuthGrant,
};

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub fn build_auth_headers(auth: &AuthConfig) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    for pair in &auth.headers {
        let name = HeaderName::from_bytes(pair.name.as_bytes())
            .map_err(|_| AppError::bad(format!("invalid header name {}", pair.name)))?;
        let value = HeaderValue::from_str(&pair.value)
            .map_err(|_| AppError::bad(format!("invalid header value for {}", pair.name)))?;
        headers.insert(name, value);
    }
    match auth.kind {
        AuthKind::None => {
            headers.remove(AUTHORIZATION);
        }
        AuthKind::Bearer => {
            let token = required_secret(auth.bearer_token.as_deref(), "bearer token is not set")?;
            headers.insert(AUTHORIZATION, header_value(&format!("Bearer {token}"))?);
        }
        AuthKind::Basic => {
            let username = auth.basic_username.as_deref().unwrap_or("").trim();
            if username.is_empty() {
                return Err(AppError::auth("basic username is not set"));
            }
            let password = auth.basic_password.as_deref().unwrap_or("");
            let encoded = STANDARD.encode(format!("{username}:{password}"));
            headers.insert(AUTHORIZATION, header_value(&format!("Basic {encoded}"))?);
        }
        AuthKind::Headers => {}
        AuthKind::OAuth2 => {
            let token = auth
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.access_token.as_deref())
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    AppError::auth("oauth access token is missing; authorize this server")
                })?;
            headers.insert(AUTHORIZATION, header_value(&format!("Bearer {token}"))?);
        }
    }
    Ok(headers)
}

fn required_secret<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, AppError> {
    value
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AppError::auth(message))
}

fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value)
        .map_err(|_| AppError::bad("credential header is not a valid HTTP value"))
}

pub async fn ensure_oauth_token(http: &Client, oauth: &mut OAuthConfig) -> Result<(), AppError> {
    let now = crate::model::chrono_lite::unix_now();
    match oauth.grant {
        OAuthGrant::ClientCredentials => {
            if oauth.access_token.as_ref().is_none_or(|t| t.is_empty())
                || token_needs_refresh(oauth.expires_at, now)
            {
                client_credentials(http, oauth).await?;
            }
        }
        OAuthGrant::AuthorizationCode => {
            if oauth.access_token.as_ref().is_none_or(|t| t.is_empty()) {
                return Err(AppError::auth(
                    "oauth authorization is required before this server can be used",
                ));
            }
            if token_needs_refresh(oauth.expires_at, now) {
                refresh_or_reauth(http, oauth).await?;
            }
        }
    }
    Ok(())
}

pub async fn force_refresh(http: &Client, oauth: &mut OAuthConfig) -> Result<(), AppError> {
    match oauth.grant {
        OAuthGrant::ClientCredentials => client_credentials(http, oauth).await,
        OAuthGrant::AuthorizationCode => refresh_or_reauth(http, oauth).await,
    }
}

async fn refresh_or_reauth(http: &Client, oauth: &mut OAuthConfig) -> Result<(), AppError> {
    if oauth
        .refresh_token
        .as_ref()
        .is_some_and(|token| !token.is_empty())
    {
        refresh_token(http, oauth).await
    } else {
        Err(AppError::auth(
            "oauth access token expired and no refresh token is stored",
        ))
    }
}

pub async fn client_credentials(http: &Client, oauth: &mut OAuthConfig) -> Result<(), AppError> {
    if oauth.token_url.trim().is_empty() {
        return Err(AppError::auth("oauth token url is not set"));
    }
    let mut params = vec![
        ("grant_type", "client_credentials".to_string()),
        ("client_id", oauth.client_id.clone()),
    ];
    if let Some(secret) = oauth.client_secret.clone().filter(|s| !s.is_empty()) {
        params.push(("client_secret", secret));
    }
    push_optional(&mut params, oauth);
    let body = token_request(
        http,
        &oauth.token_url,
        &params,
        oauth.client_secret.as_deref(),
    )
    .await?;
    apply_token_response(oauth, &body);
    if oauth.access_token.as_ref().is_none_or(|t| t.is_empty()) {
        return Err(AppError::auth(
            "token endpoint did not return an access token",
        ));
    }
    Ok(())
}

pub async fn refresh_token(http: &Client, oauth: &mut OAuthConfig) -> Result<(), AppError> {
    let refresh = oauth
        .refresh_token
        .clone()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AppError::auth("refresh token is missing"))?;
    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh),
        ("client_id", oauth.client_id.clone()),
    ];
    if let Some(secret) = oauth.client_secret.clone().filter(|s| !s.is_empty()) {
        params.push(("client_secret", secret));
    }
    push_optional(&mut params, oauth);
    let body = token_request(
        http,
        &oauth.token_url,
        &params,
        oauth.client_secret.as_deref(),
    )
    .await?;
    apply_token_response(oauth, &body);
    Ok(())
}

pub async fn exchange_code(
    http: &Client,
    oauth: &mut OAuthConfig,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<(), AppError> {
    let mut params = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("client_id", oauth.client_id.clone()),
        ("code_verifier", verifier.to_string()),
    ];
    if let Some(secret) = oauth.client_secret.clone().filter(|s| !s.is_empty()) {
        params.push(("client_secret", secret));
    }
    push_optional(&mut params, oauth);
    let body = token_request(
        http,
        &oauth.token_url,
        &params,
        oauth.client_secret.as_deref(),
    )
    .await?;
    apply_token_response(oauth, &body);
    if oauth.access_token.as_ref().is_none_or(|t| t.is_empty()) {
        return Err(AppError::auth(
            "token endpoint did not return an access token",
        ));
    }
    Ok(())
}

fn push_optional(params: &mut Vec<(&str, String)>, oauth: &OAuthConfig) {
    if !oauth.scopes.is_empty() {
        params.push(("scope", oauth.scopes.join(" ")));
    }
    if let Some(resource) = oauth.resource.as_ref().filter(|value| !value.is_empty()) {
        params.push(("resource", resource.clone()));
    }
}

async fn token_request(
    http: &Client,
    token_url: &str,
    params: &[(&str, String)],
    client_secret: Option<&str>,
) -> Result<Value, AppError> {
    let body = form_body(params);
    let first = post_form(http, token_url, body.clone(), None).await?;
    if first.status == 401 {
        if let Some(secret) = client_secret.filter(|s| !s.is_empty()) {
            let client_id = params
                .iter()
                .find(|(key, _)| *key == "client_id")
                .map(|(_, value)| value.as_str())
                .unwrap_or("");
            let basic = STANDARD.encode(format!("{client_id}:{secret}"));
            let stripped: Vec<(&str, String)> = params
                .iter()
                .filter(|(key, _)| *key != "client_secret")
                .cloned()
                .collect();
            let second = post_form(http, token_url, form_body(&stripped), Some(&basic)).await?;
            return interpret_token(second);
        }
    }
    interpret_token(first)
}

struct TokenHttp {
    status: reqwest::StatusCode,
    body: String,
}

async fn post_form(
    http: &Client,
    token_url: &str,
    body: String,
    basic: Option<&str>,
) -> Result<TokenHttp, AppError> {
    let mut req = http
        .post(token_url)
        .timeout(std::time::Duration::from_secs(30))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(body);
    if let Some(basic) = basic {
        req = req.header(AUTHORIZATION, format!("Basic {basic}"));
    }
    let response = req
        .send()
        .await
        .map_err(|err| AppError::auth(format!("token endpoint: {err}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| AppError::auth(format!("token endpoint body: {err}")))?;
    Ok(TokenHttp { status, body })
}

fn interpret_token(response: TokenHttp) -> Result<Value, AppError> {
    if !response.status.is_success() {
        return Err(AppError::auth(format!(
            "token endpoint returned {}: {}",
            response.status,
            truncate(&response.body)
        )));
    }
    serde_json::from_str(&response.body)
        .map_err(|err| AppError::auth(format!("token endpoint JSON: {err}")))
}

fn form_body(params: &[(&str, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

pub fn pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 48];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

pub fn random_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn authorization_redirect(
    oauth: &OAuthConfig,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<String, AppError> {
    let raw = oauth
        .authorize_url
        .as_deref()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| AppError::bad("oauth authorize url is not set"))?;
    let mut url = Url::parse(raw).map_err(|err| AppError::bad(format!("authorize url: {err}")))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &oauth.client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("state", state);
        query.append_pair("code_challenge", challenge);
        query.append_pair("code_challenge_method", "S256");
        if !oauth.scopes.is_empty() {
            query.append_pair("scope", &oauth.scopes.join(" "));
        }
        if let Some(resource) = oauth.resource.as_ref().filter(|value| !value.is_empty()) {
            query.append_pair("resource", resource);
        }
    }
    Ok(url.to_string())
}

pub async fn register_client(
    http: &Client,
    registration_endpoint: &str,
    redirect_uri: &str,
    scopes: &[String],
) -> Result<(String, Option<String>), AppError> {
    let payload = json!({
        "client_name": "loopback-mcp-proxy",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "scope": scopes.join(" "),
    });
    let response = http
        .post(registration_endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|err| AppError::upstream(format!("client registration: {err}")))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|err| AppError::upstream(format!("client registration body: {err}")))?;
    if !status.is_success() {
        return Err(AppError::upstream(format!(
            "client registration returned {status}: {}",
            truncate(&body.to_string())
        )));
    }
    let client_id = body
        .get("client_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::upstream("registration response has no client_id"))?
        .to_string();
    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok((client_id, client_secret))
}

#[derive(Debug, Default)]
struct Challenge {
    realm: Option<String>,
    resource_metadata: Option<String>,
    scope: Option<String>,
}

pub fn parse_www_authenticate(header: &str) -> (Option<String>, Option<String>, Option<String>) {
    let challenge = parse_challenge(header);
    (
        challenge.realm,
        challenge.resource_metadata,
        challenge.scope,
    )
}

fn parse_challenge(header: &str) -> Challenge {
    let mut challenge = Challenge::default();
    let mut rest = header.trim();
    if let Some(index) = rest.find(char::is_whitespace) {
        rest = rest[index..].trim();
    }
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else {
            break;
        };
        let key = rest[..eq]
            .trim()
            .trim_matches(',')
            .trim()
            .to_ascii_lowercase();
        rest = rest[eq + 1..].trim();
        let (value, next) = if rest.starts_with('"') {
            let bytes = rest.as_bytes();
            let mut end = 1;
            while end < bytes.len() && bytes[end] != b'"' {
                end += 1;
            }
            let value = rest[1..end].to_string();
            let after = if end + 1 < rest.len() {
                end + 1
            } else {
                rest.len()
            };
            (value, rest[after..].trim_start_matches([',', ' ']))
        } else {
            let end = rest.find([',', ' ']).unwrap_or(rest.len());
            (
                rest[..end].to_string(),
                rest[end..].trim_start_matches([',', ' ']),
            )
        };
        match key.as_str() {
            "realm" => challenge.realm = Some(value),
            "resource_metadata" => challenge.resource_metadata = Some(value),
            "scope" => challenge.scope = Some(value),
            _ => {}
        }
        rest = next;
    }
    challenge
}

pub async fn discover(http: &Client, mcp_url: &str) -> Result<Discovery, AppError> {
    let probe = unauthenticated_initialize(http, mcp_url).await?;
    if probe.status.is_success() {
        return Ok(Discovery {
            auth_required: false,
            authorization_endpoint: None,
            token_endpoint: None,
            registration_endpoint: None,
            scopes: Vec::new(),
            realm: None,
            resource: Some(mcp_url.to_string()),
            message: "endpoint accepted an unauthenticated initialize".into(),
        });
    }
    let www = probe.www_authenticate.unwrap_or_default();
    let (realm, metadata_url, scope) = parse_www_authenticate(&www);
    let mut scopes = scope
        .map(|value| value.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let mut resource = Some(mcp_url.to_string());
    let mut authorization_servers: Vec<String> = Vec::new();
    if let Some(meta) = fetch_resource_metadata(http, mcp_url, metadata_url.as_deref()).await {
        if let Some(value) = meta.get("resource").and_then(|v| v.as_str()) {
            resource = Some(value.to_string());
        }
        if let Some(list) = meta.get("scopes_supported").and_then(|v| v.as_array()) {
            let parsed: Vec<String> = list
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if !parsed.is_empty() {
                scopes = parsed;
            }
        }
        if let Some(list) = meta.get("authorization_servers").and_then(|v| v.as_array()) {
            authorization_servers = list
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
        }
    }
    let mut authorization_endpoint = None;
    let mut token_endpoint = None;
    let mut registration_endpoint = None;
    if let Some(issuer) = authorization_servers.first() {
        if let Some(doc) = fetch_authorization_server(http, issuer).await {
            authorization_endpoint = doc
                .get("authorization_endpoint")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            token_endpoint = doc
                .get("token_endpoint")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            registration_endpoint = doc
                .get("registration_endpoint")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if scopes.is_empty() {
                if let Some(list) = doc.get("scopes_supported").and_then(|v| v.as_array()) {
                    scopes = list
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect();
                }
            }
        }
    }
    let message = if token_endpoint.is_some() {
        "discovered OAuth metadata from the MCP endpoint".to_string()
    } else if probe.status.as_u16() == 401 || probe.status.as_u16() == 403 {
        format!("endpoint requires authentication ({})", probe.status)
    } else {
        format!("initialize returned {}", probe.status)
    };
    Ok(Discovery {
        auth_required: probe.status.as_u16() == 401
            || probe.status.as_u16() == 403
            || token_endpoint.is_some(),
        authorization_endpoint,
        token_endpoint,
        registration_endpoint,
        scopes,
        realm,
        resource,
        message,
    })
}

struct ProbeHttp {
    status: reqwest::StatusCode,
    www_authenticate: Option<String>,
}

async fn unauthenticated_initialize(http: &Client, mcp_url: &str) -> Result<ProbeHttp, AppError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOLS[0],
            "capabilities": {},
            "clientInfo": {"name": "loopback-mcp-proxy", "version": env!("CARGO_PKG_VERSION")}
        }
    });
    let response = http
        .post(mcp_url)
        .timeout(std::time::Duration::from_secs(20))
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", PROTOCOLS[0])
        .json(&body)
        .send()
        .await
        .map_err(|err| AppError::upstream(format!("discover {mcp_url}: {err}")))?;
    let status = response.status();
    let www_authenticate = response
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    Ok(ProbeHttp {
        status,
        www_authenticate,
    })
}

async fn fetch_resource_metadata(
    http: &Client,
    mcp_url: &str,
    hinted: Option<&str>,
) -> Option<Value> {
    let mut urls = Vec::new();
    if let Some(hinted) = hinted {
        urls.push(hinted.to_string());
    }
    if let Ok(url) = Url::parse(mcp_url) {
        let origin = origin_of(&url);
        let path = url.path().trim_end_matches('/');
        if !path.is_empty() && path != "/" {
            urls.push(format!(
                "{origin}/.well-known/oauth-protected-resource{path}"
            ));
        }
        urls.push(format!("{origin}/.well-known/oauth-protected-resource"));
    }
    for url in urls {
        if let Some(value) = get_json(http, &url).await {
            return Some(value);
        }
    }
    None
}

async fn fetch_authorization_server(http: &Client, issuer: &str) -> Option<Value> {
    for suffix in ["oauth-authorization-server", "openid-configuration"] {
        for url in well_known_urls(issuer, suffix) {
            if let Some(value) = get_json(http, &url).await {
                if value.get("token_endpoint").is_some()
                    || value.get("authorization_endpoint").is_some()
                {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn well_known_urls(issuer: &str, suffix: &str) -> Vec<String> {
    let Ok(url) = Url::parse(issuer) else {
        return Vec::new();
    };
    let origin = origin_of(&url);
    let path = url.path().trim_end_matches('/');
    let mut urls = vec![format!("{origin}/.well-known/{suffix}")];
    if !path.is_empty() && path != "/" {
        urls.push(format!("{origin}/.well-known/{suffix}{path}"));
    }
    urls.push(format!(
        "{}/.well-known/{suffix}",
        issuer.trim_end_matches('/')
    ));
    urls
}

fn origin_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

async fn get_json(http: &Client, url: &str) -> Option<Value> {
    let response = http.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

fn truncate(value: &str) -> String {
    let compact: String = value.chars().take(280).collect();
    compact.replace(['\n', '\r'], " ")
}

pub fn apply_discovery(oauth: &mut OAuthConfig, discovery: &Discovery) {
    if oauth.authorize_url.as_ref().is_none_or(|s| s.is_empty()) {
        oauth.authorize_url = discovery.authorization_endpoint.clone();
    }
    if oauth.token_url.is_empty() {
        oauth.token_url = discovery.token_endpoint.clone().unwrap_or_default();
    }
    if oauth.registration_endpoint.is_none() {
        oauth.registration_endpoint = discovery.registration_endpoint.clone();
    }
    if oauth.scopes.is_empty() {
        oauth.scopes = discovery.scopes.clone();
    }
    if oauth.resource.is_none() {
        oauth.resource = discovery.resource.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_header_uses_standard_base64() {
        let auth = AuthConfig {
            kind: AuthKind::Basic,
            bearer_token: None,
            basic_username: Some("user".into()),
            basic_password: Some("pass".into()),
            basic_realm: Some("mcp".into()),
            headers: Vec::new(),
            oauth: None,
        };
        let headers = build_auth_headers(&auth).unwrap();
        let value = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert_eq!(value, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn parses_bearer_challenge() {
        let header = r#"Bearer realm="mcp", resource_metadata="https://example.com/.well-known/oauth-protected-resource", scope="mcp.read""#;
        let (realm, metadata, scope) = parse_www_authenticate(header);
        assert_eq!(realm.as_deref(), Some("mcp"));
        assert_eq!(
            metadata.as_deref(),
            Some("https://example.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(scope.as_deref(), Some("mcp.read"));
    }

    #[test]
    fn pkce_challenge_is_s256_length() {
        let (verifier, challenge) = pkce_pair();
        assert!(verifier.len() >= 43);
        assert_eq!(challenge.len(), 43);
    }
}
