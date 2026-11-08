use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::AppError;
use crate::model::Transport;
use crate::sse::{SseEvent, SseParser};

const ACCEPT_VALUE: &str = "application/json, text/event-stream";

#[derive(Clone)]
pub struct Upstream {
    inner: Arc<StdMutex<Inner>>,
    next_id: Arc<AtomicU64>,
    http: Client,
}

struct Inner {
    url: String,
    transport: Transport,
    protocol: String,
    session_id: Option<String>,
    info: Value,
    sse_post: Option<String>,
    sse_pending: Option<Arc<StdMutex<HashMap<String, oneshot::Sender<Value>>>>>,
    sse_alive: Arc<AtomicBool>,
    _task: Option<TaskGuard>,
}

struct TaskGuard(JoinHandle<()>);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Upstream {
    pub async fn connect(
        http: &Client,
        transport: Transport,
        url: &str,
        headers: &HeaderMap,
    ) -> Result<Self, AppError> {
        let upstream = Self {
            inner: Arc::new(StdMutex::new(Inner {
                url: url.to_string(),
                transport,
                protocol: "2025-06-18".into(),
                session_id: None,
                info: Value::Null,
                sse_post: None,
                sse_pending: None,
                sse_alive: Arc::new(AtomicBool::new(true)),
                _task: None,
            })),
            next_id: Arc::new(AtomicU64::new(1)),
            http: http.clone(),
        };
        match transport {
            Transport::Sse => upstream.open_sse(headers).await?,
            Transport::StreamableHttp => {}
        }
        upstream.initialize(headers).await?;
        Ok(upstream)
    }

    pub async fn list_tools(&self, headers: &HeaderMap) -> Result<Vec<Value>, AppError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params = json!({});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let result = self.rpc("tools/list", params, headers, true).await?;
            if let Some(list) = result.get("tools").and_then(|value| value.as_array()) {
                tools.extend(list.clone());
            }
            cursor = result
                .get("nextCursor")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        headers: &HeaderMap,
        name: &str,
        arguments: Value,
    ) -> Result<Value, AppError> {
        let arguments = if arguments.is_null() {
            json!({})
        } else {
            arguments
        };
        self.rpc(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
            headers,
            true,
        )
        .await
    }

    pub async fn request(
        &self,
        headers: &HeaderMap,
        method: &str,
        params: Value,
    ) -> Result<Value, AppError> {
        let params = if params.is_null() { json!({}) } else { params };
        self.rpc(method, params, headers, true).await
    }

    pub fn info(&self) -> Value {
        self.lock().info.clone()
    }

    async fn initialize(&self, headers: &HeaderMap) -> Result<(), AppError> {
        let protocols = ["2025-06-18", "2025-03-26", "2024-11-05"];
        let mut last_error = None;
        for protocol in protocols {
            {
                let mut inner = self.lock();
                inner.protocol = protocol.to_string();
            }
            let params = json!({
                "protocolVersion": protocol,
                "capabilities": { "roots": { "listChanged": false } },
                "clientInfo": {
                    "name": "loopback-mcp-proxy",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            match self.rpc("initialize", params, headers, true).await {
                Ok(result) => {
                    if let Some(agreed) = result.get("protocolVersion").and_then(|v| v.as_str()) {
                        self.lock().protocol = agreed.to_string();
                    }
                    self.lock().info = result;
                    self.notify("notifications/initialized", json!({}), headers)
                        .await?;
                    return Ok(());
                }
                Err(err) if err.is_auth() || err.is_session() => return Err(err),
                Err(err) => {
                    let message = err.message.to_ascii_lowercase();
                    if message.contains("protocol") {
                        last_error = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::upstream("initialize failed")))
    }

    async fn notify(
        &self,
        method: &str,
        params: Value,
        headers: &HeaderMap,
    ) -> Result<(), AppError> {
        self.rpc(method, params, headers, false).await.map(|_| ())
    }

    async fn rpc(
        &self,
        method: &str,
        params: Value,
        headers: &HeaderMap,
        expect_result: bool,
    ) -> Result<Value, AppError> {
        let id = if expect_result {
            Some(Value::from(self.next_id.fetch_add(1, Ordering::Relaxed)))
        } else {
            None
        };
        let mut message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        if let Some(id) = &id {
            message["id"] = id.clone();
        }
        let (url, transport, protocol, session, pending, sse_alive) = {
            let inner = self.lock();
            (
                inner.sse_post.clone().unwrap_or_else(|| inner.url.clone()),
                inner.transport,
                inner.protocol.clone(),
                inner.session_id.clone(),
                inner.sse_pending.clone(),
                inner.sse_alive.clone(),
            )
        };
        if transport == Transport::Sse && !sse_alive.load(Ordering::Relaxed) {
            return Err(AppError::session("SSE stream disconnected"));
        }
        let waiter = match (&id, &pending, transport) {
            (Some(id), Some(pending), Transport::Sse) => {
                let (tx, rx) = oneshot::channel();
                pending
                    .lock()
                    .map_err(|_| AppError::internal("sse map poisoned"))?
                    .insert(id_key(id), tx);
                Some(rx)
            }
            _ => None,
        };
        let response = self
            .post_json(&url, headers, &protocol, session.as_deref(), message)
            .await;
        let response = match response {
            Ok(response) => response,
            Err(err) => {
                if let (Some(id), Some(pending)) = (&id, &pending) {
                    if let Ok(mut map) = pending.lock() {
                        map.remove(&id_key(id));
                    }
                }
                return Err(err);
            }
        };
        if let Some(session_id) = response.session_id.clone() {
            self.lock().session_id = Some(session_id);
        }
        if response.status.as_u16() == 401 || response.status.as_u16() == 403 {
            let realm = response
                .www_authenticate
                .as_deref()
                .and_then(|header| crate::auth::parse_www_authenticate(header).0);
            let mut message = format!("upstream rejected credentials ({})", response.status);
            if let Some(realm) = realm {
                message.push_str(&format!("; realm=\"{realm}\""));
            }
            if !response.body_text.is_empty() {
                message.push_str(": ");
                message.push_str(&truncate(&response.body_text));
            }
            return Err(AppError::auth(message));
        }
        if response.status.as_u16() == 404 {
            return Err(AppError::session(format!(
                "upstream session expired: {}",
                truncate(&response.body_text)
            )));
        }
        if !response.status.is_success() {
            return Err(AppError::upstream(format!(
                "upstream {} for {method}: {}",
                response.status,
                truncate(&response.body_text)
            )));
        }
        if let Some(value) = response.json.clone() {
            if let Some(error) = value.get("error") {
                let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-32000);
                let message = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("upstream error");
                if code == -32602 && message.to_ascii_lowercase().contains("protocol") {
                    return Err(AppError::upstream(message));
                }
                return Err(AppError::upstream(format!("{message} ({code})")));
            }
            if !expect_result {
                return Ok(Value::Null);
            }
            if value.get("result").is_some() || value.get("id").is_some() {
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
        }
        if !expect_result {
            return Ok(Value::Null);
        }
        if let Some(rx) = waiter {
            let value = tokio::time::timeout(Duration::from_secs(170), rx)
                .await
                .map_err(|_| AppError::upstream("timed out waiting for an SSE response"))?
                .map_err(|_| {
                    AppError::upstream("SSE connection closed before a response arrived")
                })?;
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("upstream error");
                return Err(AppError::upstream(message));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
        Err(AppError::upstream(format!(
            "upstream returned an empty body for {method}"
        )))
    }

    async fn post_json(
        &self,
        url: &str,
        headers: &HeaderMap,
        protocol: &str,
        session: Option<&str>,
        message: Value,
    ) -> Result<PostResponse, AppError> {
        let mut request = self
            .http
            .post(url)
            .timeout(Duration::from_secs(180))
            .header(ACCEPT, ACCEPT_VALUE)
            .header(CONTENT_TYPE, "application/json")
            .header("mcp-protocol-version", protocol)
            .headers(headers.clone())
            .json(&message);
        if let Some(session) = session {
            request = request.header("mcp-session-id", session);
        }
        let response = request
            .send()
            .await
            .map_err(|err| AppError::upstream(format!("request to {url}: {err}")))?;
        let status = response.status();
        let session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let www_authenticate = response
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("text/event-stream") {
            let id = message.get("id").cloned();
            let json = read_sse_response(response, id.as_ref()).await?;
            return Ok(PostResponse {
                status,
                session_id,
                www_authenticate,
                json,
                body_text: String::new(),
            });
        }
        let body_text = response
            .text()
            .await
            .map_err(|err| AppError::upstream(format!("read body: {err}")))?;
        let json = if body_text.trim().is_empty() {
            None
        } else {
            match serde_json::from_str::<Value>(&body_text) {
                Ok(value) => Some(value),
                Err(_) if status.is_success() && message.get("id").is_none() => None,
                Err(err) => {
                    return Err(AppError::upstream(format!(
                        "upstream JSON ({err}): {}",
                        truncate(&body_text)
                    )));
                }
            }
        };
        Ok(PostResponse {
            status,
            session_id,
            www_authenticate,
            json,
            body_text,
        })
    }

    async fn open_sse(&self, headers: &HeaderMap) -> Result<(), AppError> {
        let (url, http, sse_alive) = {
            let inner = self.lock();
            inner.sse_alive.store(true, Ordering::Relaxed);
            (
                inner.url.clone(),
                self.http.clone(),
                inner.sse_alive.clone(),
            )
        };
        let (endpoint_tx, endpoint_rx) = oneshot::channel();
        let pending: Arc<StdMutex<HashMap<String, oneshot::Sender<Value>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let pending_task = pending.clone();
        let headers = headers.clone();
        let task_url = url.clone();
        let task = tokio::spawn(async move {
            let response = match http
                .get(&task_url)
                .header(ACCEPT, "text/event-stream")
                .headers(headers)
                .send()
                .await
            {
                Ok(response) => response,
                Err(_) => {
                    sse_alive.store(false, Ordering::Relaxed);
                    return;
                }
            };
            if !response.status().is_success() {
                sse_alive.store(false, Ordering::Relaxed);
                return;
            }
            let mut stream = response.bytes_stream();
            let mut parser = SseParser::default();
            let mut endpoint_tx = Some(endpoint_tx);
            while let Some(chunk) = stream.next().await {
                let Ok(chunk) = chunk else { break };
                let text = String::from_utf8_lossy(&chunk);
                for event in parser.push(&text) {
                    if endpoint_tx.is_some() && is_endpoint_event(&event) {
                        if let Some(tx) = endpoint_tx.take() {
                            let _ = tx.send(event.data.clone());
                        }
                        continue;
                    }
                    deliver_sse(&pending_task, &event);
                }
            }
            sse_alive.store(false, Ordering::Relaxed);
            if let Ok(mut waiters) = pending_task.lock() {
                waiters.clear();
            }
        });
        let endpoint = tokio::time::timeout(Duration::from_secs(20), endpoint_rx)
            .await
            .map_err(|_| AppError::upstream("timed out waiting for the SSE endpoint event"))?
            .map_err(|_| AppError::upstream("SSE connection closed before the endpoint event"))?;
        let post_url = resolve_endpoint(&url, &endpoint)?;
        let mut inner = self.lock();
        inner.sse_post = Some(post_url);
        inner.sse_pending = Some(pending);
        inner._task = Some(TaskGuard(task));
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("upstream lock")
    }
}

struct PostResponse {
    status: reqwest::StatusCode,
    session_id: Option<String>,
    www_authenticate: Option<String>,
    json: Option<Value>,
    body_text: String,
}

fn is_endpoint_event(event: &SseEvent) -> bool {
    event.event == "endpoint" || event.data.starts_with('/') || event.data.starts_with("http")
}

fn deliver_sse(pending: &StdMutex<HashMap<String, oneshot::Sender<Value>>>, event: &SseEvent) {
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return;
    };
    let Some(id) = value.get("id") else {
        return;
    };
    if let Ok(mut map) = pending.lock() {
        if let Some(tx) = map.remove(&id_key(id)) {
            let _ = tx.send(value);
        }
    }
}

async fn read_sse_response(
    response: reqwest::Response,
    id: Option<&Value>,
) -> Result<Option<Value>, AppError> {
    let mut stream = response.bytes_stream();
    let mut parser = SseParser::default();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(170);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let chunk = tokio::time::timeout(remaining, stream.next())
            .await
            .map_err(|_| AppError::upstream("timed out reading an SSE response"))?;
        let Some(chunk) = chunk else {
            break;
        };
        let chunk = chunk.map_err(|err| AppError::upstream(format!("sse read: {err}")))?;
        let text = String::from_utf8_lossy(&chunk);
        for event in parser.push(&text) {
            let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
                continue;
            };
            match id {
                Some(expected)
                    if value
                        .get("id")
                        .is_some_and(|got| id_key(got) == id_key(expected)) =>
                {
                    return Ok(Some(value));
                }
                None => return Ok(Some(value)),
                _ => {}
            }
        }
    }
    Err(AppError::upstream(
        "SSE response ended before a matching JSON-RPC message",
    ))
}

fn resolve_endpoint(base: &str, endpoint: &str) -> Result<String, AppError> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return Ok(endpoint.to_string());
    }
    let base =
        reqwest::Url::parse(base).map_err(|err| AppError::upstream(format!("base url: {err}")))?;
    base.join(endpoint)
        .map(|url| url.to_string())
        .map_err(|err| AppError::upstream(format!("sse endpoint: {err}")))
}

fn id_key(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn truncate(value: &str) -> String {
    value
        .chars()
        .take(300)
        .collect::<String>()
        .replace(['\n', '\r'], " ")
}
