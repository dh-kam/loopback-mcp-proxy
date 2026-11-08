use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::Catalog;
use crate::error::AppError;
use crate::model::{namespaced_tool, split_namespaced};

const MAX_SESSIONS: usize = 2048;
const SESSION_TTL: Duration = Duration::from_secs(24 * 60 * 60);

struct SessionEntry {
    scope: Option<String>,
    last_seen: Instant,
}

#[derive(Default)]
pub struct SessionStore {
    inner: Mutex<HashMap<String, SessionEntry>>,
}

impl SessionStore {
    pub fn open(&self, scope: Option<String>) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Instant::now();
        if let Ok(mut guard) = self.inner.lock() {
            if guard.len() >= MAX_SESSIONS {
                guard.retain(|_, v| now.duration_since(v.last_seen) < SESSION_TTL);
                if guard.len() >= MAX_SESSIONS {
                    if let Some(oldest_key) = guard
                        .iter()
                        .min_by_key(|(_, v)| v.last_seen)
                        .map(|(k, _)| k.clone())
                    {
                        guard.remove(&oldest_key);
                    }
                }
            }
            guard.insert(
                id.clone(),
                SessionEntry {
                    scope,
                    last_seen: now,
                },
            );
        }
        id
    }

    pub fn scope(&self, id: &str) -> Option<Option<String>> {
        let now = Instant::now();
        self.inner.lock().ok().and_then(|mut guard| {
            let entry = guard.get_mut(id)?;
            if now.duration_since(entry.last_seen) >= SESSION_TTL {
                guard.remove(id);
                return None;
            }
            entry.last_seen = now;
            Some(entry.scope.clone())
        })
    }

    pub fn close(&self, id: &str) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(id))
            .is_some()
    }
}

pub struct McpResponse {
    pub status: StatusCode,
    pub session_id: Option<String>,
    pub body: Option<Value>,
}

pub async fn dispatch(
    catalog: &Catalog,
    sessions: &SessionStore,
    scope: Option<&str>,
    session_header: Option<&str>,
    payload: Value,
) -> McpResponse {
    if let Value::Array(items) = payload {
        let mut responses = Vec::new();
        let mut session_id = session_header.map(str::to_string);
        for item in items {
            let response =
                dispatch_one(catalog, sessions, scope, session_id.as_deref(), item).await;
            if session_id.is_none() {
                session_id = response.session_id.clone();
            }
            if let Some(body) = response.body {
                responses.push(body);
            }
        }
        return McpResponse {
            status: if responses.is_empty() {
                StatusCode::ACCEPTED
            } else {
                StatusCode::OK
            },
            session_id,
            body: if responses.is_empty() {
                None
            } else {
                Some(Value::Array(responses))
            },
        };
    }
    dispatch_one(catalog, sessions, scope, session_header, payload).await
}

async fn dispatch_one(
    catalog: &Catalog,
    sessions: &SessionStore,
    scope: Option<&str>,
    session_header: Option<&str>,
    payload: Value,
) -> McpResponse {
    let method = payload
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let id = payload.get("id").cloned();
    let params = payload.get("params").cloned().unwrap_or(Value::Null);
    if method.is_empty() {
        return json_response(None, rpc_error(id, -32600, "invalid request"));
    }
    if method.starts_with("notifications/") {
        return McpResponse {
            status: StatusCode::ACCEPTED,
            session_id: session_header.map(str::to_string),
            body: None,
        };
    }
    if method != "initialize" {
        if session_header.is_none() {
            return McpResponse {
                status: StatusCode::BAD_REQUEST,
                session_id: None,
                body: Some(rpc_error(id, -32600, "mcp-session-id header is required")),
            };
        }
        let Some(saved_scope) = sessions.scope(session_header.unwrap_or_default()) else {
            return McpResponse {
                status: StatusCode::NOT_FOUND,
                session_id: None,
                body: Some(rpc_error(id, -32001, "session not found")),
            };
        };
        if saved_scope.as_deref() != scope {
            return McpResponse {
                status: StatusCode::NOT_FOUND,
                session_id: None,
                body: Some(rpc_error(id, -32001, "session scope mismatch")),
            };
        }
    }
    let result = match method {
        "initialize" => {
            let (server_info, instructions) = if let Some(target) = scope {
                let Ok(server_id) = catalog.id_for_slug(target).await else {
                    return McpResponse {
                        status: StatusCode::NOT_FOUND,
                        session_id: None,
                        body: Some(rpc_error(
                            id,
                            -32001,
                            &format!("upstream `{target}` not found"),
                        )),
                    };
                };
                let Ok(view) = catalog.view(&server_id).await else {
                    return McpResponse {
                        status: StatusCode::NOT_FOUND,
                        session_id: None,
                        body: Some(rpc_error(
                            id,
                            -32001,
                            &format!("upstream `{target}` not found"),
                        )),
                    };
                };
                let upstream_info = view
                    .server_info
                    .as_ref()
                    .and_then(|info| info.get("serverInfo"))
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "name": format!("loopback-mcp-proxy/{}", view.slug),
                            "version": env!("CARGO_PKG_VERSION")
                        })
                    });
                let inst = format!(
                    "Tools from upstream `{}` ({}) are exposed with their original names. Credentials are isolated inside loopback-mcp-proxy.",
                    view.slug, view.name
                );
                (upstream_info, inst)
            } else {
                (
                    json!({ "name": "loopback-mcp-proxy", "version": env!("CARGO_PKG_VERSION") }),
                    "Tools from every enabled upstream are exposed as {slug}__{tool}. This endpoint does not require credentials.".to_string(),
                )
            };
            let client_name = params
                .get("clientInfo")
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("mcp-client");
            let session = sessions.open(scope.map(str::to_string));
            catalog
                .record_activity(
                    None,
                    scope.unwrap_or("all"),
                    "mcp",
                    "handshake",
                    "ok",
                    "initialize",
                    None,
                    Some(0),
                    true,
                    format!(
                        "local client `{client_name}` initialized session on {}",
                        scope
                            .map(|s| format!("/mcp/{s}"))
                            .unwrap_or_else(|| "/mcp".into())
                    ),
                    None,
                    None,
                )
                .await;
            let requested_protocol = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-06-18");
            let agreed_protocol = match requested_protocol {
                "2025-06-18" | "2025-03-26" | "2024-11-05" => requested_protocol,
                _ => "2025-06-18",
            };
            let body = rpc_ok(
                id,
                json!({
                    "protocolVersion": agreed_protocol,
                    "capabilities": {
                        "tools": { "listChanged": true },
                        "resources": { "subscribe": false, "listChanged": false },
                        "prompts": { "listChanged": false }
                    },
                    "serverInfo": server_info,
                    "instructions": instructions
                }),
            );
            return McpResponse {
                status: StatusCode::OK,
                session_id: Some(session),
                body: Some(body),
            };
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let tools = tool_list(catalog, scope).await;
            catalog
                .record_activity(
                    None,
                    scope.unwrap_or("all"),
                    "mcp",
                    "handshake",
                    "ok",
                    "tools/list",
                    None,
                    Some(0),
                    true,
                    format!(
                        "local client listed {} tools from {}",
                        tools.len(),
                        scope
                            .map(|s| format!("/mcp/{s}"))
                            .unwrap_or_else(|| "/mcp".into())
                    ),
                    None,
                    None,
                )
                .await;
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => call_tool(catalog, scope, &params).await,
        "resources/list" => list_resources(catalog, scope, &params).await,
        "resources/templates/list" => list_resource_templates(catalog, scope, &params).await,
        "resources/read" => read_resource(catalog, scope, &params).await,
        "prompts/list" => list_prompts(catalog, scope, &params).await,
        "prompts/get" => get_prompt(catalog, scope, &params).await,
        "logging/setLevel" => Ok(json!({})),
        _ => Err(AppError::bad(format!("method not found: {method}"))),
    };
    match result {
        Ok(value) => json_response(session_header.map(str::to_string), rpc_ok(id, value)),
        Err(err) => {
            let code = if err.message.starts_with("method not found") {
                -32601
            } else if err.status == StatusCode::BAD_REQUEST {
                -32602
            } else {
                -32000
            };
            json_response(
                session_header.map(str::to_string),
                rpc_error(id, code, &err.message),
            )
        }
    }
}

async fn tool_list(catalog: &Catalog, scope: Option<&str>) -> Vec<Value> {
    let views = catalog.views().await;
    let mut tools = Vec::new();
    for view in views {
        if view.state != crate::model::ConnState::Connected {
            continue;
        }
        let allowed_tools = view.tool_policy.filter_tools(&view.tools);
        if let Some(scope) = scope {
            if view.slug != scope && view.id != scope {
                continue;
            }
            tools.extend(allowed_tools);
            continue;
        }
        for tool in allowed_tools {
            tools.push(namespaced_tool(&view.slug, &tool));
        }
    }
    tools
}

async fn call_tool(
    catalog: &Catalog,
    scope: Option<&str>,
    params: &Value,
) -> Result<Value, AppError> {
    let name = params
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AppError::bad("tools/call requires a name"))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let (server_key, tool_name) = if let Some(scope) = scope {
        (scope.to_string(), name)
    } else {
        let slugs: Vec<String> = catalog
            .views()
            .await
            .into_iter()
            .map(|view| view.slug)
            .collect();
        let (slug, tool) = split_namespaced(name, &slugs).ok_or_else(|| {
            AppError::bad(format!(
                "tool `{name}` is not namespaced as {{slug}}__{{tool}}"
            ))
        })?;
        (slug.to_string(), tool)
    };
    let id = catalog.id_for_slug(&server_key).await?;
    catalog
        .call_tool_sourced(&id, tool_name, arguments, crate::catalog::CallSource::Mcp)
        .await
}

async fn list_resources(
    catalog: &Catalog,
    scope: Option<&str>,
    params: &Value,
) -> Result<Value, AppError> {
    if let Some(target) = scope {
        let id = catalog.id_for_slug(target).await?;
        return match catalog
            .forward_rpc(&id, "resources/list", params.clone())
            .await
        {
            Ok(val) => Ok(val),
            Err(_) => Ok(json!({ "resources": [] })),
        };
    }
    let views = catalog.views().await;
    let mut resources = Vec::new();
    for view in views {
        if view.state != crate::model::ConnState::Connected {
            continue;
        }
        if let Ok(val) = catalog
            .forward_rpc(&view.id, "resources/list", json!({}))
            .await
        {
            if let Some(items) = val.get("resources").and_then(Value::as_array) {
                for item in items {
                    let mut cloned = item.clone();
                    if let Some(obj) = cloned.as_object_mut() {
                        if let Some(uri) = obj.get("uri").and_then(Value::as_str) {
                            obj.insert("uri".into(), json!(format!("mcp://{}/{}", view.slug, uri)));
                        }
                        if let Some(name) = obj.get("name").and_then(Value::as_str) {
                            obj.insert("name".into(), json!(format!("[{}] {}", view.slug, name)));
                        }
                    }
                    resources.push(cloned);
                }
            }
        }
    }
    Ok(json!({ "resources": resources }))
}

async fn list_resource_templates(
    catalog: &Catalog,
    scope: Option<&str>,
    params: &Value,
) -> Result<Value, AppError> {
    if let Some(target) = scope {
        let id = catalog.id_for_slug(target).await?;
        return match catalog
            .forward_rpc(&id, "resources/templates/list", params.clone())
            .await
        {
            Ok(val) => Ok(val),
            Err(_) => Ok(json!({ "resourceTemplates": [] })),
        };
    }
    let views = catalog.views().await;
    let mut templates = Vec::new();
    for view in views {
        if view.state != crate::model::ConnState::Connected {
            continue;
        }
        if let Ok(val) = catalog
            .forward_rpc(&view.id, "resources/templates/list", json!({}))
            .await
        {
            if let Some(items) = val.get("resourceTemplates").and_then(Value::as_array) {
                for item in items {
                    let mut cloned = item.clone();
                    if let Some(obj) = cloned.as_object_mut() {
                        if let Some(tpl) = obj.get("uriTemplate").and_then(Value::as_str) {
                            obj.insert(
                                "uriTemplate".into(),
                                json!(format!("mcp://{}/{}", view.slug, tpl)),
                            );
                        }
                        if let Some(name) = obj.get("name").and_then(Value::as_str) {
                            obj.insert("name".into(), json!(format!("[{}] {}", view.slug, name)));
                        }
                    }
                    templates.push(cloned);
                }
            }
        }
    }
    Ok(json!({ "resourceTemplates": templates }))
}

async fn read_resource(
    catalog: &Catalog,
    scope: Option<&str>,
    params: &Value,
) -> Result<Value, AppError> {
    if let Some(target) = scope {
        let id = catalog.id_for_slug(target).await?;
        return catalog
            .forward_rpc(&id, "resources/read", params.clone())
            .await;
    }
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad("resources/read requires a uri"))?;
    let rest = uri.strip_prefix("mcp://").ok_or_else(|| {
        AppError::bad("aggregated resources/read requires a `mcp://{slug}/{uri}` URI")
    })?;
    let (slug, inner_uri) = rest.split_once('/').ok_or_else(|| {
        AppError::bad("aggregated resources/read requires a `mcp://{slug}/{uri}` URI")
    })?;
    let id = catalog.id_for_slug(slug).await?;
    let mut forwarded = params.clone();
    if let Some(obj) = forwarded.as_object_mut() {
        obj.insert("uri".into(), json!(inner_uri));
    }
    catalog.forward_rpc(&id, "resources/read", forwarded).await
}

async fn list_prompts(
    catalog: &Catalog,
    scope: Option<&str>,
    params: &Value,
) -> Result<Value, AppError> {
    if let Some(target) = scope {
        let id = catalog.id_for_slug(target).await?;
        return match catalog
            .forward_rpc(&id, "prompts/list", params.clone())
            .await
        {
            Ok(val) => Ok(val),
            Err(_) => Ok(json!({ "prompts": [] })),
        };
    }
    let views = catalog.views().await;
    let mut prompts = Vec::new();
    for view in views {
        if view.state != crate::model::ConnState::Connected {
            continue;
        }
        if let Ok(val) = catalog.forward_rpc(&view.id, "prompts/list", json!({})).await {
            if let Some(items) = val.get("prompts").and_then(Value::as_array) {
                for item in items {
                    prompts.push(namespaced_tool(&view.slug, item));
                }
            }
        }
    }
    Ok(json!({ "prompts": prompts }))
}

async fn get_prompt(
    catalog: &Catalog,
    scope: Option<&str>,
    params: &Value,
) -> Result<Value, AppError> {
    if let Some(target) = scope {
        let id = catalog.id_for_slug(target).await?;
        return catalog
            .forward_rpc(&id, "prompts/get", params.clone())
            .await;
    }
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad("prompts/get requires a name"))?;
    let slugs: Vec<String> = catalog
        .views()
        .await
        .into_iter()
        .map(|view| view.slug)
        .collect();
    let (slug, prompt_name) = split_namespaced(name, &slugs).ok_or_else(|| {
        AppError::bad(format!(
            "prompt `{name}` is not namespaced as {{slug}}__{{prompt}}"
        ))
    })?;
    let id = catalog.id_for_slug(slug).await?;
    let mut forwarded = params.clone();
    if let Some(obj) = forwarded.as_object_mut() {
        obj.insert("name".into(), json!(prompt_name));
    }
    catalog.forward_rpc(&id, "prompts/get", forwarded).await
}

fn json_response(session_id: Option<String>, body: Value) -> McpResponse {
    McpResponse {
        status: StatusCode::OK,
        session_id,
        body: Some(body),
    }
}

fn rpc_ok(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}

fn rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}
