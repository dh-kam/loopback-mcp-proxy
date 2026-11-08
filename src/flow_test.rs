use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

use crate::config::Config;
use crate::http::{router, AppState};
use crate::model::{AuthConfig, AuthKind, OAuthConfig, OAuthGrant, ServerInput, Transport};

struct HitLog {
    auths: Mutex<Vec<String>>,
    grants: Mutex<Vec<String>>,
}

async fn spawn(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{address}")
}

fn bearer_mock(expected: &str) -> (Router, Arc<HitLog>) {
    let log = Arc::new(HitLog {
        auths: Mutex::new(Vec::new()),
        grants: Mutex::new(Vec::new()),
    });
    let expected = expected.to_string();
    let state = (log.clone(), expected);
    let app = Router::new()
        .route("/mcp", post(bearer_mcp))
        .with_state(state);
    (app, log)
}

async fn bearer_mcp(
    State((log, expected)): State<(Arc<HitLog>, String)>,
    headers: HeaderMap,
    body: String,
) -> axum::response::Response {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    log.auths.lock().unwrap().push(auth.clone());
    if auth != format!("Bearer {expected}") {
        return (
            StatusCode::UNAUTHORIZED,
            [(
                axum::http::header::WWW_AUTHENTICATE,
                "Bearer realm=\"mock\"",
            )],
            "unauthorized",
        )
            .into_response();
    }
    mcp_dispatch(&headers, &body)
}

fn oauth_mock() -> (Router, Arc<HitLog>) {
    let log = Arc::new(HitLog {
        auths: Mutex::new(Vec::new()),
        grants: Mutex::new(Vec::new()),
    });
    let app = Router::new()
        .route("/mcp", post(oauth_mcp))
        .route("/token", post(token))
        .with_state(log.clone());
    (app, log)
}

async fn token(State(log): State<Arc<HitLog>>, body: String) -> Json<Value> {
    let grant = url::form_urlencoded::parse(body.as_bytes())
        .find(|(key, _)| key == "grant_type")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    log.grants.lock().unwrap().push(grant);
    let n = log.grants.lock().unwrap().len();
    let access = if n == 1 { "tok-a" } else { "tok-b" };
    Json(json!({
        "access_token": access,
        "refresh_token": "ref-1",
        "expires_in": 0,
        "token_type": "Bearer"
    }))
}

async fn oauth_mcp(
    State(log): State<Arc<HitLog>>,
    headers: HeaderMap,
    body: String,
) -> axum::response::Response {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    log.auths.lock().unwrap().push(auth);
    mcp_dispatch(&headers, &body)
}

fn mcp_dispatch(headers: &HeaderMap, body: &str) -> axum::response::Response {
    let payload: Value = serde_json::from_str(body).unwrap_or(json!({}));
    let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = payload.get("id").cloned().unwrap_or(Value::Null);
    if method.starts_with("notifications/") {
        return StatusCode::ACCEPTED.into_response();
    }
    if method != "initialize" && headers.get("mcp-session-id").is_none() {
        return (StatusCode::BAD_REQUEST, "missing session").into_response();
    }
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "mock", "version": "1" }
        }),
        "tools/list" => json!({
            "tools": [{
                "name": "echo",
                "description": "Echo arguments",
                "inputSchema": {
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"]
                }
            }]
        }),
        "tools/call" => {
            let args = payload
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(json!({}));
            json!({
                "content": [{ "type": "text", "text": args.to_string() }],
                "isError": false
            })
        }
        "resources/list" => json!({
            "resources": [{
                "uri": "file:///readme.md",
                "name": "README",
                "mimeType": "text/markdown"
            }]
        }),
        "resources/read" => {
            let uri = payload
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or("");
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": format!("content of {uri}")
                }]
            })
        }
        "prompts/list" => json!({
            "prompts": [{
                "name": "summarize",
                "description": "Summarize document"
            }]
        }),
        "prompts/get" => {
            let name = payload
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            json!({
                "description": format!("prompt {name}"),
                "messages": [{
                    "role": "user",
                    "content": { "type": "text", "text": format!("run {name}") }
                }]
            })
        }
        _ => json!({}),
    };
    let mut response = Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response();
    if method == "initialize" {
        response
            .headers_mut()
            .insert("mcp-session-id", "sess-1".parse().unwrap());
    }
    response
}

fn proxy_config(dir: &std::path::Path) -> Config {
    Config {
        bind: "127.0.0.1:9".parse().unwrap(),
        data_dir: dir.to_path_buf(),
        public_base_url: "http://127.0.0.1:9".into(),
        admin_token: Some("admin-secret".into()),
        allow_non_loopback: false,
    }
}

async fn boot_proxy(mut config: Config) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    config.bind = address;
    config.public_base_url = format!("http://{address}");
    let app = AppState::bootstrap(config).await.unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router(app)).await.unwrap();
    });
    format!("http://{address}")
}

fn admin(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder.bearer_auth("admin-secret")
}

#[tokio::test]
async fn bearer_proxy_hides_credentials_and_calls_tools() {
    let (mock, log) = bearer_mock("secret-token");
    let upstream = spawn(mock).await;
    let dir = tempdir().unwrap();
    let proxy = boot_proxy(proxy_config(dir.path())).await;
    let http = reqwest::Client::new();

    let denied = http
        .post(format!("{proxy}/api/servers"))
        .json(&sample(&upstream, "secret-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let created = admin(http.post(format!("{proxy}/api/servers")))
        .json(&sample(&upstream, "secret-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body = created.text().await.unwrap();
    assert!(!created_body.contains("secret-token"));
    let view: Value = serde_json::from_str(&created_body).unwrap();
    assert_eq!(view["state"], "connected");
    assert_eq!(view["toolCount"], 1);
    assert_eq!(view["auth"]["bearerConfigured"], true);
    let id = view["id"].as_str().unwrap();

    let direct = admin(http.post(format!("{proxy}/api/servers/{id}/tools/call")))
        .json(&json!({"name": "echo", "arguments": {"text": "ping"}}))
        .send()
        .await
        .unwrap();
    assert!(direct.status().is_success());
    let direct_body = direct.text().await.unwrap();
    assert!(direct_body.contains("ping"));

    let init = http
        .post(format!("{proxy}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }
        }))
        .send()
        .await
        .unwrap();
    assert!(init.status().is_success());
    let session = init
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let listed = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &session)
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(listed["result"]["tools"][0]["name"], "lab__echo");

    let called = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &session)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "lab__echo", "arguments": { "text": "through-proxy" } }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(called["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("through-proxy"));
    assert!(log
        .auths
        .lock()
        .unwrap()
        .iter()
        .all(|auth| auth == "Bearer secret-token"));

    let updated = admin(http.put(format!("{proxy}/api/servers/{id}")))
        .json(&sample(&upstream, ""))
        .send()
        .await
        .unwrap();
    assert!(updated.status().is_success());
    let again = admin(http.post(format!("{proxy}/api/servers/{id}/tools/call")))
        .json(&json!({"name": "echo", "arguments": {"text": "kept"}}))
        .send()
        .await
        .unwrap();
    assert!(
        again.status().is_success(),
        "{}",
        again.text().await.unwrap()
    );

    // Changing upstream origin with blank secret must be rejected to prevent secret exfiltration
    let (evil_mock, evil_log) = bearer_mock("secret-token");
    let evil_upstream = spawn(evil_mock).await;
    let mut probe_body = serde_json::to_value(sample(&evil_upstream, "")).unwrap();
    probe_body["serverId"] = json!(id);
    let evil_probe = admin(http.post(format!("{proxy}/api/probe")))
        .json(&probe_body)
        .send()
        .await
        .unwrap();
    assert_eq!(evil_probe.status(), StatusCode::BAD_REQUEST);
    let evil_update = admin(http.put(format!("{proxy}/api/servers/{id}")))
        .json(&sample(&evil_upstream, ""))
        .send()
        .await
        .unwrap();
    assert_eq!(evil_update.status(), StatusCode::BAD_REQUEST);
    let mut evil_probe_call_body = probe_body.clone();
    evil_probe_call_body["toolName"] = json!("echo");
    evil_probe_call_body["arguments"] = json!({"text": "exfil"});
    let evil_probe_call = admin(http.post(format!("{proxy}/api/probe/call")))
        .json(&evil_probe_call_body)
        .send()
        .await
        .unwrap();
    assert_eq!(evil_probe_call.status(), StatusCode::BAD_REQUEST);
    assert!(evil_log.auths.lock().unwrap().is_empty());

    // Ephemeral /api/probe/call works before/without saving or with same-origin stored secret
    let mut valid_probe_call = serde_json::to_value(sample(&upstream, "")).unwrap();
    valid_probe_call["serverId"] = json!(id);
    valid_probe_call["toolName"] = json!("echo");
    valid_probe_call["arguments"] = json!({"text": "probe-bench"});
    let probe_called = admin(http.post(format!("{proxy}/api/probe/call")))
        .json(&valid_probe_call)
        .send()
        .await
        .unwrap();
    assert!(probe_called.status().is_success());
    let probe_called_json: Value = probe_called.json().await.unwrap();
    assert_eq!(probe_called_json["ok"], true);
    assert!(probe_called_json["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("probe-bench"));

    // Scoped endpoint /mcp/lab exposes original tool names and upstream serverInfo
    let scoped_init = http
        .post(format!("{proxy}/mcp/lab"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test-cli", "version": "0"}
            }
        }))
        .send()
        .await
        .unwrap();
    assert!(scoped_init.status().is_success());
    let scoped_session = scoped_init
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let scoped_init_body: Value = scoped_init.json().await.unwrap();
    assert_eq!(scoped_init_body["result"]["serverInfo"]["name"], "mock");

    let scoped_tools = http
        .post(format!("{proxy}/mcp/lab"))
        .header("mcp-session-id", &scoped_session)
        .json(&json!({"jsonrpc":"2.0","id":11,"method":"tools/list"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(scoped_tools["result"]["tools"][0]["name"], "echo");

    // Verify resources/* and prompts/* forwarding on both /mcp (namespaced) and /mcp/lab (passthrough)
    let agg_resources = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &session)
        .json(&json!({"jsonrpc":"2.0","id":20,"method":"resources/list"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        agg_resources["result"]["resources"][0]["uri"],
        "mcp://lab/file:///readme.md"
    );

    let agg_read = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &session)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "resources/read",
            "params": { "uri": "mcp://lab/file:///readme.md" }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        agg_read["result"]["contents"][0]["text"],
        "content of file:///readme.md"
    );

    let agg_prompts = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &session)
        .json(&json!({"jsonrpc":"2.0","id":22,"method":"prompts/list"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        agg_prompts["result"]["prompts"][0]["name"],
        "lab__summarize"
    );

    let agg_prompt_get = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &session)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 23,
            "method": "prompts/get",
            "params": { "name": "lab__summarize" }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        agg_prompt_get["result"]["messages"][0]["content"]["text"],
        "run summarize"
    );

    let scoped_resources = http
        .post(format!("{proxy}/mcp/lab"))
        .header("mcp-session-id", &scoped_session)
        .json(&json!({"jsonrpc":"2.0","id":24,"method":"resources/list"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        scoped_resources["result"]["resources"][0]["uri"],
        "file:///readme.md"
    );

    // Cross-scope session reuse is rejected
    let cross_scope = http
        .post(format!("{proxy}/mcp"))
        .header("mcp-session-id", &scoped_session)
        .json(&json!({"jsonrpc":"2.0","id":12,"method":"tools/list"}))
        .send()
        .await
        .unwrap();
    assert_eq!(cross_scope.status(), StatusCode::NOT_FOUND);

    // Updating toolPolicy blocks disallowed tools on downstream /mcp/lab while keeping admin ToolBench calls working
    let policy_updated = admin(http.put(format!("{proxy}/api/servers/{id}/policy")))
        .json(&json!({
            "mode": "denylist",
            "allowedTools": [],
            "deniedTools": ["echo"]
        }))
        .send()
        .await
        .unwrap();
    assert!(policy_updated.status().is_success());
    let policy_view: Value = policy_updated.json().await.unwrap();
    assert_eq!(policy_view["exposedToolCount"], 0);
    assert_eq!(policy_view["toolCount"], 1);

    let scoped_tools_after = http
        .post(format!("{proxy}/mcp/lab"))
        .header("mcp-session-id", &scoped_session)
        .json(&json!({"jsonrpc":"2.0","id":13,"method":"tools/list"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        scoped_tools_after["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let blocked_call = http
        .post(format!("{proxy}/mcp/lab"))
        .header("mcp-session-id", &scoped_session)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": { "name": "echo", "arguments": { "text": "blocked" } }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(blocked_call["error"]["message"]
        .as_str()
        .unwrap()
        .contains("blocked by `lab` security policy"));
}

#[tokio::test]
async fn oauth_client_credentials_refresh_before_tool_call() {
    let (mock, log) = oauth_mock();
    let upstream = spawn(mock).await;
    let dir = tempdir().unwrap();
    let proxy = boot_proxy(proxy_config(dir.path())).await;
    let http = reqwest::Client::new();
    let input = ServerInput {
        name: "OAuth".into(),
        slug: "oauth".into(),
        url: format!("{upstream}/mcp"),
        transport: Transport::StreamableHttp,
        enabled: true,
        auth: AuthConfig {
            kind: AuthKind::OAuth2,
            bearer_token: None,
            basic_username: None,
            basic_password: None,
            basic_realm: None,
            headers: Vec::new(),
            oauth: Some(OAuthConfig {
                grant: OAuthGrant::ClientCredentials,
                client_id: "client".into(),
                client_secret: Some("secret".into()),
                authorize_url: None,
                token_url: format!("{upstream}/token"),
                scopes: vec!["mcp".into()],
                access_token: None,
                refresh_token: None,
                expires_at: None,
                resource: None,
                registration_endpoint: None,
            }),
        },
        tool_policy: None,
    };
    let created = admin(http.post(format!("{proxy}/api/servers")))
        .json(&input)
        .send()
        .await
        .unwrap();
    let status = created.status();
    let body = created.text().await.unwrap();
    assert!(status.is_success(), "{body}");
    assert!(!body.contains("tok-a"), "{body}");
    let view: Value = serde_json::from_str(&body).unwrap();
    let id = view["id"].as_str().unwrap();

    let called = admin(http.post(format!("{proxy}/api/servers/{id}/tools/call")))
        .json(&json!({"name": "echo", "arguments": {"text": "renewed"}}))
        .send()
        .await
        .unwrap();
    let called_status = called.status();
    let called_body = called.text().await.unwrap();
    assert!(called_status.is_success(), "{called_body}");
    assert!(called_body.contains("renewed"));
    let grants = log.grants.lock().unwrap().clone();
    assert!(grants.len() >= 2, "{grants:?}");
    assert!(grants.iter().all(|grant| grant == "client_credentials"));
    let auths = log.auths.lock().unwrap().clone();
    assert!(auths.iter().any(|auth| auth == "Bearer tok-a"), "{auths:?}");
    assert!(auths.last().unwrap() == "Bearer tok-b", "{auths:?}");
}

#[tokio::test]
async fn import_parse_endpoint_handles_cli_commands_and_configs() {
    let dir = tempdir().unwrap();
    let proxy = boot_proxy(proxy_config(dir.path())).await;
    let http = reqwest::Client::new();

    for (cmd, expected_slug, expected_url) in [
        (
            "agy plugin install notion@antigravity-plugins-partners",
            "notion",
            "https://mcp.notion.com/mcp",
        ),
        (
            "claude plugin install linear",
            "linear",
            "https://mcp.linear.app/mcp",
        ),
        (
            r#"claude mcp add --transport http github https://api.githubcopilot.com/mcp/ --header "Authorization: Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}""#,
            "github",
            "https://api.githubcopilot.com/mcp/",
        ),
        (
            "codex mcp add context7 --url https://mcp.context7.com/mcp",
            "context7",
            "https://mcp.context7.com/mcp",
        ),
        (
            "npx -y @notionhq/notion-mcp-server",
            "notion",
            "https://mcp.notion.com/mcp",
        ),
    ] {
        let resp = admin(http.post(format!("{proxy}/api/import/parse")))
            .json(&json!({
                "command": cmd,
                "autoDiscover": false
            }))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success(), "failed for {cmd}");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["candidates"][0]["input"]["slug"], expected_slug);
        assert_eq!(body["candidates"][0]["input"]["url"], expected_url);
    }
}

#[tokio::test]
async fn vault_export_import_and_auth_clear_work() {
    let (mock, _log) = bearer_mock("secret-token");
    let upstream = spawn(mock).await;
    let dir1 = tempdir().unwrap();
    let proxy1 = boot_proxy(proxy_config(dir1.path())).await;
    let http = reqwest::Client::new();

    let created: Value = admin(http.post(format!("{proxy1}/api/servers")))
        .json(&sample(&upstream, "secret-token"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["auth"]["bearerConfigured"], true);

    // Export encrypted vault bundle
    let bundle: Value = admin(http.post(format!("{proxy1}/api/vault/export")))
        .json(&json!({ "passphrase": "correct-horse-battery" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bundle["format"], "loopback-mcp-vault-v1");
    assert_eq!(bundle["serverCount"], 1);
    assert!(!bundle.to_string().contains("secret-token"));

    // Clear auth tokens on proxy1
    let cleared: Value = admin(http.post(format!("{proxy1}/api/servers/{id}/auth/clear")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cleared["auth"]["bearerConfigured"], false);
    assert_eq!(cleared["state"], "authRequired");

    // Import bundle into a brand-new proxy2 instance
    let dir2 = tempdir().unwrap();
    let proxy2 = boot_proxy(proxy_config(dir2.path())).await;
    let imported: Value = admin(http.post(format!("{proxy2}/api/vault/import")))
        .json(&json!({
            "passphrase": "correct-horse-battery",
            "bundle": bundle
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(imported[0]["slug"], "lab");
    assert_eq!(imported[0]["auth"]["bearerConfigured"], true);
    assert_eq!(imported[0]["state"], "connected");
}

#[tokio::test]
async fn security_boundary_blocks_csrf_dns_rebinding_ssrf_and_self_recursion() {
    let dir = tempdir().unwrap();
    let proxy = boot_proxy(proxy_config(dir.path())).await;
    let http = reqwest::Client::new();

    // 1. Self-recursion guard: upstream pointing back at proxy's own port
    let self_loop = admin(http.post(format!("{proxy}/api/servers")))
        .json(&sample(&proxy, "secret-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(self_loop.status(), StatusCode::BAD_REQUEST);
    let self_loop_body = self_loop.text().await.unwrap();
    assert!(
        self_loop_body.contains("self-recursion"),
        "{self_loop_body}"
    );

    // 2. Cloud metadata SSRF guard
    let ssrf = admin(http.post(format!("{proxy}/api/servers")))
        .json(&sample("http://169.254.169.254/latest", "secret-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(ssrf.status(), StatusCode::BAD_REQUEST);
    let ssrf_body = ssrf.text().await.unwrap();
    assert!(ssrf_body.contains("SSRF"), "{ssrf_body}");

    // 3. Browser CSRF guard on /mcp (Origin: https://evil.example)
    let csrf_mcp = http
        .post(format!("{proxy}/mcp"))
        .header("Origin", "https://evil.example")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .send()
        .await
        .unwrap();
    assert_eq!(csrf_mcp.status(), StatusCode::FORBIDDEN);

    // 4. Browser CSRF guard via Sec-Fetch-Site: cross-site
    let csrf_fetch_site = http
        .post(format!("{proxy}/mcp"))
        .header("Sec-Fetch-Site", "cross-site")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .send()
        .await
        .unwrap();
    assert_eq!(csrf_fetch_site.status(), StatusCode::FORBIDDEN);

    // 5. DNS Rebinding guard on /mcp (Host: evil.rebind.attacker.com:8080)
    let dns_rebind = http
        .post(format!("{proxy}/mcp"))
        .header("Host", "evil.rebind.attacker.com:8080")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .send()
        .await
        .unwrap();
    assert_eq!(dns_rebind.status(), StatusCode::FORBIDDEN);

    // 6. Same-origin Web UI request is allowed
    let same_origin = admin(
        http.get(format!("{proxy}/api/servers"))
            .header("Origin", &proxy)
            .header("Sec-Fetch-Site", "same-origin"),
    )
    .send()
    .await
    .unwrap();
    assert!(same_origin.status().is_success());
}

fn sample(upstream: &str, token: &str) -> ServerInput {
    ServerInput {
        name: "Lab".into(),
        slug: "lab".into(),
        url: format!("{upstream}/mcp"),
        transport: Transport::StreamableHttp,
        enabled: true,
        auth: AuthConfig {
            kind: AuthKind::Bearer,
            bearer_token: if token.is_empty() {
                None
            } else {
                Some(token.into())
            },
            basic_username: None,
            basic_password: None,
            basic_realm: Some("mock".into()),
            headers: Vec::new(),
            oauth: None,
        },
        tool_policy: None,
    }
}
