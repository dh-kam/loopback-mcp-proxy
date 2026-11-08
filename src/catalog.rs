use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};

use crate::auth::{build_auth_headers, ensure_oauth_token, force_refresh};
use crate::error::AppError;
use crate::model::{
    chrono_lite, redact_and_truncate_json, summarize_tool_result, ActivityEntry, AuthKind,
    BusEvent, ConnState, Server, ServerInput, ServerView,
};
use crate::store::Store;
use crate::upstream::Upstream;

const MAX_ACTIVITY_LOG: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallSource {
    Admin,
    Mcp,
    Probe,
}

impl CallSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Mcp => "mcp",
            Self::Probe => "probe",
        }
    }
}

pub struct Catalog {
    inner: RwLock<HashMap<String, LiveServer>>,
    activity_seq: AtomicU64,
    activity_log: RwLock<VecDeque<ActivityEntry>>,
    http: Client,
    store: Arc<Store>,
    events: broadcast::Sender<BusEvent>,
}

struct LiveServer {
    server: Server,
    state: ConnState,
    message: String,
    info: Option<Value>,
    tools: Vec<Value>,
    upstream: Option<Upstream>,
    backoff_secs: u64,
    next_attempt: Option<Instant>,
    last_tool_sync: Option<Instant>,
    checked_at: Option<String>,
    total_calls: u64,
    error_count: u64,
    last_latency_ms: Option<u64>,
}

impl LiveServer {
    fn idle(server: Server) -> Self {
        let state = if server.enabled {
            ConnState::Idle
        } else {
            ConnState::Disabled
        };
        Self {
            server,
            state,
            message: String::new(),
            info: None,
            tools: Vec::new(),
            upstream: None,
            backoff_secs: 2,
            next_attempt: None,
            last_tool_sync: None,
            checked_at: None,
            total_calls: 0,
            error_count: 0,
            last_latency_ms: None,
        }
    }

    fn view(&self) -> ServerView {
        let exposed_tool_count = self.server.tool_policy.filter_tools(&self.tools).len();
        ServerView {
            id: self.server.id.clone(),
            name: self.server.name.clone(),
            slug: self.server.slug.clone(),
            url: self.server.url.clone(),
            transport: self.server.transport,
            enabled: self.server.enabled,
            auth: self.server.auth.public_view(),
            tool_policy: self.server.tool_policy.clone(),
            state: self.state,
            message: self.message.clone(),
            tool_count: self.tools.len(),
            exposed_tool_count,
            tools: self.tools.clone(),
            server_info: self.info.clone(),
            created_at: self.server.created_at.clone(),
            updated_at: self.server.updated_at.clone(),
            checked_at: self.checked_at.clone(),
            total_calls: self.total_calls,
            error_count: self.error_count,
            last_latency_ms: self.last_latency_ms,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub ok: bool,
    pub duration_ms: u64,
    pub message: String,
    pub server_info: Option<Value>,
    pub tools: Vec<Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeCallResult {
    pub ok: bool,
    pub duration_ms: u64,
    pub message: String,
    pub result: Option<Value>,
}

impl Catalog {
    pub fn new(store: Arc<Store>, http: Client, events: broadcast::Sender<BusEvent>) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            activity_seq: AtomicU64::new(1),
            activity_log: RwLock::new(VecDeque::with_capacity(MAX_ACTIVITY_LOG)),
            http,
            store,
            events,
        }
    }

    pub async fn load(&self) -> Result<(), AppError> {
        let servers = self.store.list()?;
        let mut guard = self.inner.write().await;
        guard.clear();
        for server in servers {
            guard.insert(server.id.clone(), LiveServer::idle(server));
        }
        drop(guard);
        if let Ok(persisted) = self.store.load_recent_activity(MAX_ACTIVITY_LOG) {
            let max_seq = persisted.iter().map(|e| e.seq).max().unwrap_or(0);
            self.activity_seq
                .store(max_seq.saturating_add(1), Ordering::Relaxed);
            let mut log = self.activity_log.write().await;
            log.clear();
            for entry in persisted {
                log.push_back(entry);
            }
        }
        Ok(())
    }

    pub async fn recent_activity(&self) -> Vec<ActivityEntry> {
        self.activity_log.read().await.iter().cloned().collect()
    }

    pub async fn clear_activity_log(&self) -> Result<(), AppError> {
        self.store.clear_activity()?;
        self.activity_log.write().await.clear();
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_activity(
        &self,
        server_id: Option<String>,
        slug: impl Into<String>,
        source: &str,
        kind: &str,
        phase: &str,
        method: &str,
        tool: Option<String>,
        duration_ms: Option<u64>,
        ok: bool,
        message: impl Into<String>,
        request_preview: Option<String>,
        response_preview: Option<String>,
    ) {
        let fallback_seq = self.activity_seq.fetch_add(1, Ordering::Relaxed);
        let slug_str = slug.into();
        let mut entry = ActivityEntry {
            seq: fallback_seq,
            timestamp: chrono_lite::rfc3339_now(),
            server_id: server_id.clone(),
            slug: slug_str.clone(),
            source: source.to_string(),
            kind: kind.to_string(),
            phase: phase.to_string(),
            method: method.to_string(),
            tool,
            duration_ms,
            ok,
            message: message.into(),
            request_preview,
            response_preview,
        };
        if let Ok(db_seq) = self.store.append_activity(&entry) {
            entry.seq = db_seq;
            self.activity_seq
                .fetch_max(db_seq.saturating_add(1), Ordering::Relaxed);
        }
        if (phase == "ok" || phase == "error")
            && (kind == "tool" || kind == "rpc" || kind == "policy")
        {
            if let Some(sid) = &server_id {
                let updated_view = {
                    let mut guard = self.inner.write().await;
                    if let Some(live) = guard.get_mut(sid) {
                        live.total_calls = live.total_calls.saturating_add(1);
                        if !ok {
                            live.error_count = live.error_count.saturating_add(1);
                        }
                        if let Some(ms) = duration_ms {
                            live.last_latency_ms = Some(ms);
                        }
                        Some(live.view())
                    } else {
                        None
                    }
                };
                if let Some(view) = updated_view {
                    let _ = self.events.send(BusEvent::Upsert { server: view });
                }
            }
        }
        {
            let mut log = self.activity_log.write().await;
            if log.len() >= MAX_ACTIVITY_LOG {
                log.pop_front();
            }
            log.push_back(entry.clone());
        }
        let _ = self.events.send(BusEvent::Activity { entry });
    }

    pub async fn views(&self) -> Vec<ServerView> {
        let guard = self.inner.read().await;
        let mut views: Vec<_> = guard.values().map(LiveServer::view).collect();
        views.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.name.cmp(&b.name)));
        views
    }

    pub async fn view(&self, id: &str) -> Result<ServerView, AppError> {
        self.inner
            .read()
            .await
            .get(id)
            .map(LiveServer::view)
            .ok_or_else(|| AppError::not_found("server not found"))
    }

    pub async fn remember(&self, server: Server) {
        let mut guard = self.inner.write().await;
        match guard.get_mut(&server.id) {
            Some(live) => {
                live.server = server;
                live.upstream = None;
                live.tools.clear();
                live.info = None;
                live.last_tool_sync = None;
            }
            None => {
                let id = server.id.clone();
                guard.insert(id, LiveServer::idle(server));
            }
        }
    }

    pub async fn update_tool_policy(&self, server: Server) -> Result<ServerView, AppError> {
        let view = {
            let mut guard = self.inner.write().await;
            let live = guard
                .get_mut(&server.id)
                .ok_or_else(|| AppError::not_found("server not found"))?;
            live.server = server.clone();
            live.view()
        };
        let _ = self.events.send(BusEvent::Upsert {
            server: view.clone(),
        });
        self.record_activity(
            Some(view.id.clone()),
            &view.slug,
            "admin",
            "policy",
            "ok",
            "policy/update",
            None,
            None,
            true,
            format!(
                "updated tool policy ({:?}: {}/{} tools exposed)",
                view.tool_policy.mode, view.exposed_tool_count, view.tool_count
            ),
            None,
            None,
        )
        .await;
        Ok(view)
    }

    pub async fn clear_server_auth(&self, id: &str) -> Result<ServerView, AppError> {
        let server = self.store.clear_auth_tokens(id)?;
        let view = {
            let mut guard = self.inner.write().await;
            let live = guard
                .get_mut(&server.id)
                .ok_or_else(|| AppError::not_found("server not found"))?;
            live.server = server.clone();
            live.upstream = None;
            live.state = if server.auth.kind == AuthKind::None {
                ConnState::Idle
            } else {
                ConnState::AuthRequired
            };
            live.message = "stored credentials cleared from vault".to_string();
            live.view()
        };
        let _ = self.events.send(BusEvent::Upsert {
            server: view.clone(),
        });
        self.record_activity(
            Some(view.id.clone()),
            &view.slug,
            "admin",
            "oauth",
            "ok",
            "auth/clear",
            None,
            None,
            true,
            "cleared stored credentials & tokens from vault",
            None,
            None,
        )
        .await;
        Ok(view)
    }

    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let slug = self
            .server(id)
            .await
            .map(|s| s.slug)
            .unwrap_or_else(|_| id.to_string());
        self.store.delete(id)?;
        self.inner.write().await.remove(id);
        let _ = self.events.send(BusEvent::Remove { id: id.to_string() });
        self.record_activity(
            Some(id.to_string()),
            slug,
            "admin",
            "state",
            "ok",
            "delete",
            None,
            None,
            true,
            "upstream removed from vault",
            None,
            None,
        )
        .await;
        Ok(())
    }

    pub async fn connect_id(&self, id: &str) -> Result<ServerView, AppError> {
        let server_meta = self.server(id).await?;
        if !server_meta.enabled {
            self.finish(id, ConnState::Disabled, "disabled", None, Vec::new(), None)
                .await;
            self.record_activity(
                Some(id.to_string()),
                &server_meta.slug,
                "system",
                "state",
                "ok",
                "disable",
                None,
                None,
                true,
                "upstream disabled",
                None,
                None,
            )
            .await;
            return self.view(id).await;
        }
        self.set_state(id, ConnState::Connecting, "connecting")
            .await;
        let started = Instant::now();
        self.record_activity(
            Some(id.to_string()),
            &server_meta.slug,
            "system",
            "handshake",
            "start",
            "initialize",
            None,
            None,
            true,
            format!(
                "connecting to {} ({})",
                server_meta.url,
                server_meta.transport.as_str()
            ),
            None,
            None,
        )
        .await;
        let (server, headers) = match self.prepare(id).await {
            Ok(ready) => ready,
            Err(err) if err.is_auth() => {
                self.finish(
                    id,
                    ConnState::AuthRequired,
                    &err.message,
                    None,
                    Vec::new(),
                    None,
                )
                .await;
                self.record_activity(
                    Some(id.to_string()),
                    &server_meta.slug,
                    "system",
                    "handshake",
                    "error",
                    "initialize",
                    None,
                    Some(started.elapsed().as_millis() as u64),
                    false,
                    &err.message,
                    None,
                    None,
                )
                .await;
                return self.view(id).await;
            }
            Err(err) => {
                self.fail(id, &err.message).await;
                self.record_activity(
                    Some(id.to_string()),
                    &server_meta.slug,
                    "system",
                    "handshake",
                    "error",
                    "initialize",
                    None,
                    Some(started.elapsed().as_millis() as u64),
                    false,
                    &err.message,
                    None,
                    None,
                )
                .await;
                return self.view(id).await;
            }
        };
        match Upstream::connect(&self.http, server.transport, &server.url, &headers).await {
            Ok(upstream) => {
                let list_started = Instant::now();
                match upstream.list_tools(&headers).await {
                    Ok(tools) => {
                        let info = upstream.info();
                        let count = tools.len();
                        let names: Vec<String> = tools
                            .iter()
                            .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
                            .take(6)
                            .collect();
                        let names_suffix = if names.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", names.join(", "))
                        };
                        self.finish(
                            id,
                            ConnState::Connected,
                            "",
                            Some(upstream),
                            tools,
                            Some(info.clone()),
                        )
                        .await;
                        self.record_activity(
                            Some(id.to_string()),
                            &server.slug,
                            "system",
                            "handshake",
                            "ok",
                            "tools/list",
                            None,
                            Some(started.elapsed().as_millis() as u64),
                            true,
                            format!("handshake verified · {} tools ready{}", count, names_suffix),
                            None,
                            Some(redact_and_truncate_json(&info, 220)),
                        )
                        .await;
                        let _ = list_started;
                    }
                    Err(err) if err.is_auth() => {
                        self.finish(
                            id,
                            ConnState::AuthRequired,
                            &err.message,
                            None,
                            Vec::new(),
                            None,
                        )
                        .await;
                        self.record_activity(
                            Some(id.to_string()),
                            &server.slug,
                            "system",
                            "handshake",
                            "error",
                            "tools/list",
                            None,
                            Some(started.elapsed().as_millis() as u64),
                            false,
                            &err.message,
                            None,
                            None,
                        )
                        .await;
                    }
                    Err(err) => {
                        self.fail(id, &err.message).await;
                        self.record_activity(
                            Some(id.to_string()),
                            &server.slug,
                            "system",
                            "handshake",
                            "error",
                            "tools/list",
                            None,
                            Some(started.elapsed().as_millis() as u64),
                            false,
                            &err.message,
                            None,
                            None,
                        )
                        .await;
                    }
                }
            }
            Err(err) if err.is_auth() => {
                self.finish(
                    id,
                    ConnState::AuthRequired,
                    &err.message,
                    None,
                    Vec::new(),
                    None,
                )
                .await;
                self.record_activity(
                    Some(id.to_string()),
                    &server.slug,
                    "system",
                    "handshake",
                    "error",
                    "initialize",
                    None,
                    Some(started.elapsed().as_millis() as u64),
                    false,
                    &err.message,
                    None,
                    None,
                )
                .await;
            }
            Err(err) => {
                self.fail(id, &err.message).await;
                self.record_activity(
                    Some(id.to_string()),
                    &server.slug,
                    "system",
                    "handshake",
                    "error",
                    "initialize",
                    None,
                    Some(started.elapsed().as_millis() as u64),
                    false,
                    &err.message,
                    None,
                    None,
                )
                .await;
            }
        }
        self.view(id).await
    }

    pub async fn call_tool(
        &self,
        id: &str,
        name: &str,
        arguments: Value,
    ) -> Result<Value, AppError> {
        self.call_tool_sourced(id, name, arguments, CallSource::Admin)
            .await
    }

    pub async fn call_tool_sourced(
        &self,
        id: &str,
        name: &str,
        arguments: Value,
        source: CallSource,
    ) -> Result<Value, AppError> {
        let server = self.server(id).await?;
        if !server.enabled {
            return Err(AppError::bad("server is disabled"));
        }
        let req_preview = redact_and_truncate_json(&arguments, 240);
        if matches!(source, CallSource::Mcp) {
            let tool_def = {
                let guard = self.inner.read().await;
                guard.get(id).and_then(|live| {
                    live.tools
                        .iter()
                        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
                        .cloned()
                })
            };
            if !server.tool_policy.is_tool_allowed(name, tool_def.as_ref()) {
                let msg = format!(
                    "tool `{name}` is blocked by `{}` security policy ({:?})",
                    server.slug, server.tool_policy.mode
                );
                self.record_activity(
                    Some(server.id.clone()),
                    &server.slug,
                    source.as_str(),
                    "policy",
                    "error",
                    "tools/call",
                    Some(name.to_string()),
                    Some(0),
                    false,
                    &msg,
                    Some(req_preview),
                    None,
                )
                .await;
                return Err(AppError::bad(msg));
            }
        }
        let started = Instant::now();
        self.record_activity(
            Some(server.id.clone()),
            &server.slug,
            source.as_str(),
            "tool",
            "start",
            "tools/call",
            Some(name.to_string()),
            None,
            true,
            format!("invoking tool `{name}`"),
            Some(req_preview.clone()),
            None,
        )
        .await;

        let (server, headers) = self.prepare(id).await?;
        let upstream = match self.upstream_of(&server.id).await {
            Some(upstream) => upstream,
            None => {
                self.connect_id(&server.id).await?;
                let message = self.message_of(&server.id).await;
                self.upstream_of(&server.id)
                    .await
                    .ok_or_else(|| AppError::upstream(message))?
            }
        };
        let outcome = match upstream.call_tool(&headers, name, arguments.clone()).await {
            Ok(value) => Ok(value),
            Err(err) if err.is_auth() => {
                self.refresh_auth(&server.id).await?;
                let (_, headers) = self.prepare(&server.id).await?;
                let upstream = self.upstream_of(&server.id).await.ok_or_else(|| {
                    AppError::upstream("server disconnected while refreshing credentials")
                })?;
                upstream.call_tool(&headers, name, arguments).await
            }
            Err(err) if err.is_session() => {
                self.connect_id(&server.id).await?;
                let (_, headers) = self.prepare(&server.id).await?;
                let upstream = self
                    .upstream_of(&server.id)
                    .await
                    .ok_or_else(|| AppError::upstream("reconnect did not produce a session"))?;
                upstream.call_tool(&headers, name, arguments).await
            }
            Err(err) => Err(err),
        };
        let elapsed = started.elapsed().as_millis() as u64;
        match &outcome {
            Ok(value) => {
                let is_tool_err = value
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let resp_preview = summarize_tool_result(value, 240);
                self.record_activity(
                    Some(server.id.clone()),
                    &server.slug,
                    source.as_str(),
                    "tool",
                    if is_tool_err { "error" } else { "ok" },
                    "tools/call",
                    Some(name.to_string()),
                    Some(elapsed),
                    !is_tool_err,
                    format!(
                        "tool `{name}` {} in {elapsed}ms",
                        if is_tool_err {
                            "returned isError=true"
                        } else {
                            "completed"
                        }
                    ),
                    Some(req_preview),
                    Some(resp_preview),
                )
                .await;
            }
            Err(err) => {
                self.record_activity(
                    Some(server.id.clone()),
                    &server.slug,
                    source.as_str(),
                    "tool",
                    "error",
                    "tools/call",
                    Some(name.to_string()),
                    Some(elapsed),
                    false,
                    format!("tool `{name}` failed: {}", err.message),
                    Some(req_preview),
                    None,
                )
                .await;
            }
        }
        outcome
    }

    pub async fn forward_rpc(
        &self,
        id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, AppError> {
        let server = self.server(id).await?;
        if !server.enabled {
            return Err(AppError::bad("server is disabled"));
        }
        let req_preview = redact_and_truncate_json(&params, 240);
        let started = Instant::now();
        let (server, headers) = self.prepare(id).await?;
        let upstream = match self.upstream_of(&server.id).await {
            Some(upstream) => upstream,
            None => {
                self.connect_id(&server.id).await?;
                let message = self.message_of(&server.id).await;
                self.upstream_of(&server.id)
                    .await
                    .ok_or_else(|| AppError::upstream(message))?
            }
        };
        let outcome = match upstream.request(&headers, method, params.clone()).await {
            Ok(value) => Ok(value),
            Err(err) if err.is_auth() => {
                self.refresh_auth(&server.id).await?;
                let (_, headers) = self.prepare(&server.id).await?;
                let upstream = self.upstream_of(&server.id).await.ok_or_else(|| {
                    AppError::upstream("server disconnected while refreshing credentials")
                })?;
                upstream.request(&headers, method, params).await
            }
            Err(err) if err.is_session() => {
                self.connect_id(&server.id).await?;
                let (_, headers) = self.prepare(&server.id).await?;
                let upstream = self
                    .upstream_of(&server.id)
                    .await
                    .ok_or_else(|| AppError::upstream("reconnect did not produce a session"))?;
                upstream.request(&headers, method, params).await
            }
            Err(err) => Err(err),
        };
        let elapsed = started.elapsed().as_millis() as u64;
        match &outcome {
            Ok(value) => {
                let resp_preview = redact_and_truncate_json(value, 240);
                self.record_activity(
                    Some(server.id.clone()),
                    &server.slug,
                    "mcp",
                    "rpc",
                    "ok",
                    method,
                    None,
                    Some(elapsed),
                    true,
                    format!("{method} completed in {elapsed}ms"),
                    Some(req_preview),
                    Some(resp_preview),
                )
                .await;
            }
            Err(err) => {
                let is_method_not_found = err.message.contains("-32601")
                    || err.message.to_ascii_lowercase().contains("method not found");
                if !is_method_not_found {
                    self.record_activity(
                        Some(server.id.clone()),
                        &server.slug,
                        "mcp",
                        "rpc",
                        "error",
                        method,
                        None,
                        Some(elapsed),
                        false,
                        format!("{method} failed: {}", err.message),
                        Some(req_preview),
                        None,
                    )
                    .await;
                }
            }
        }
        outcome
    }

    pub async fn id_for_slug(&self, slug_or_id: &str) -> Result<String, AppError> {
        let guard = self.inner.read().await;
        if guard.contains_key(slug_or_id) {
            return Ok(slug_or_id.to_string());
        }
        guard
            .values()
            .find(|live| live.server.slug == slug_or_id)
            .map(|live| live.server.id.clone())
            .ok_or_else(|| AppError::not_found("server not found"))
    }

    pub async fn maintain(&self) {
        let now = Instant::now();
        let plan: Vec<Maintain> = {
            let guard = self.inner.read().await;
            guard
                .values()
                .map(|live| Maintain {
                    id: live.server.id.clone(),
                    enabled: live.server.enabled,
                    state: live.state,
                    next_attempt: live.next_attempt,
                    last_tool_sync: live.last_tool_sync,
                })
                .collect()
        };
        let tasks = plan.into_iter().map(|item| async move {
            if !item.enabled {
                return;
            }
            if item.state == ConnState::AuthRequired || item.state == ConnState::Connecting {
                return;
            }
            if item.state == ConnState::Connected {
                let stale = item
                    .last_tool_sync
                    .map(|at| now.duration_since(at) >= Duration::from_secs(60))
                    .unwrap_or(true);
                if stale {
                    self.sync_tools(&item.id).await;
                }
                return;
            }
            if item.next_attempt.is_some_and(|at| at > now) {
                return;
            }
            let _ = self.connect_id(&item.id).await;
        });
        futures_util::future::join_all(tasks).await;
    }

    pub async fn probe_with_logs(
        &self,
        input: &ServerInput,
        server_id: Option<&str>,
    ) -> ProbeResult {
        let started = Instant::now();
        let slug = input.slug.trim();
        let sid = server_id.map(str::to_string);
        self.record_activity(
            sid.clone(),
            slug,
            "probe",
            "handshake",
            "start",
            "initialize",
            None,
            None,
            true,
            format!(
                "probing {} ({})",
                input.url.trim(),
                input.transport.as_str()
            ),
            None,
            None,
        )
        .await;
        let result = Self::probe(&self.http, input).await;
        self.record_activity(
            sid,
            slug,
            "probe",
            "handshake",
            if result.ok { "ok" } else { "error" },
            "tools/list",
            None,
            Some(started.elapsed().as_millis() as u64),
            result.ok,
            result.message.clone(),
            None,
            result
                .server_info
                .as_ref()
                .map(|info| redact_and_truncate_json(info, 220)),
        )
        .await;
        result
    }

    pub async fn probe(http: &Client, input: &ServerInput) -> ProbeResult {
        let started = Instant::now();
        let (upstream, headers) = match Self::connect_ephemeral(http, input).await {
            Ok(pair) => pair,
            Err(err) => {
                return ProbeResult {
                    ok: false,
                    duration_ms: started.elapsed().as_millis() as u64,
                    message: err.message,
                    server_info: None,
                    tools: Vec::new(),
                };
            }
        };
        let info = upstream.info();
        match upstream.list_tools(&headers).await {
            Ok(tools) => ProbeResult {
                ok: true,
                duration_ms: started.elapsed().as_millis() as u64,
                message: format!("connected, {} tools", tools.len()),
                server_info: Some(info),
                tools,
            },
            Err(err) => ProbeResult {
                ok: false,
                duration_ms: started.elapsed().as_millis() as u64,
                message: err.message,
                server_info: Some(info),
                tools: Vec::new(),
            },
        }
    }

    pub async fn probe_call(
        &self,
        input: &ServerInput,
        server_id: Option<&str>,
        tool_name: &str,
        arguments: Value,
    ) -> ProbeCallResult {
        let started = Instant::now();
        let slug = input.slug.trim();
        let sid = server_id.map(str::to_string);
        let req_preview = redact_and_truncate_json(&arguments, 240);
        self.record_activity(
            sid.clone(),
            slug,
            "probe",
            "tool",
            "start",
            "tools/call",
            Some(tool_name.to_string()),
            None,
            true,
            format!("probe invoking tool `{tool_name}`"),
            Some(req_preview.clone()),
            None,
        )
        .await;

        let (upstream, headers) = match Self::connect_ephemeral(&self.http, input).await {
            Ok(pair) => pair,
            Err(err) => {
                let elapsed = started.elapsed().as_millis() as u64;
                self.record_activity(
                    sid,
                    slug,
                    "probe",
                    "tool",
                    "error",
                    "tools/call",
                    Some(tool_name.to_string()),
                    Some(elapsed),
                    false,
                    err.message.clone(),
                    Some(req_preview),
                    None,
                )
                .await;
                return ProbeCallResult {
                    ok: false,
                    duration_ms: elapsed,
                    message: err.message,
                    result: None,
                };
            }
        };

        match upstream.call_tool(&headers, tool_name, arguments).await {
            Ok(value) => {
                let elapsed = started.elapsed().as_millis() as u64;
                let is_tool_err = value
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let resp_preview = summarize_tool_result(&value, 240);
                self.record_activity(
                    sid,
                    slug,
                    "probe",
                    "tool",
                    if is_tool_err { "error" } else { "ok" },
                    "tools/call",
                    Some(tool_name.to_string()),
                    Some(elapsed),
                    !is_tool_err,
                    format!("probe `{tool_name}` completed in {elapsed}ms"),
                    Some(req_preview),
                    Some(resp_preview),
                )
                .await;
                ProbeCallResult {
                    ok: !is_tool_err,
                    duration_ms: elapsed,
                    message: format!("completed in {elapsed}ms"),
                    result: Some(value),
                }
            }
            Err(err) => {
                let elapsed = started.elapsed().as_millis() as u64;
                self.record_activity(
                    sid,
                    slug,
                    "probe",
                    "tool",
                    "error",
                    "tools/call",
                    Some(tool_name.to_string()),
                    Some(elapsed),
                    false,
                    err.message.clone(),
                    Some(req_preview),
                    None,
                )
                .await;
                ProbeCallResult {
                    ok: false,
                    duration_ms: elapsed,
                    message: err.message,
                    result: None,
                }
            }
        }
    }

    async fn connect_ephemeral(
        http: &Client,
        input: &ServerInput,
    ) -> Result<(Upstream, HeaderMap), AppError> {
        let mut auth = input.auth.clone();
        if auth.kind == AuthKind::OAuth2 {
            if let Some(oauth) = auth.oauth.as_mut() {
                ensure_oauth_token(http, oauth).await?;
            }
        }
        let headers = build_auth_headers(&auth)?;
        let upstream = Upstream::connect(http, input.transport, input.url.trim(), &headers).await?;
        Ok((upstream, headers))
    }

    async fn sync_tools(&self, id: &str) {
        let Some(upstream) = self.upstream_of(id).await else {
            return;
        };
        let Ok((_, headers)) = self.prepare(id).await else {
            return;
        };
        match upstream.list_tools(&headers).await {
            Ok(tools) => {
                let info = upstream.info();
                self.finish(
                    id,
                    ConnState::Connected,
                    "",
                    Some(upstream),
                    tools,
                    Some(info),
                )
                .await;
            }
            Err(err) if err.is_auth() => {
                self.finish(
                    id,
                    ConnState::AuthRequired,
                    &err.message,
                    None,
                    Vec::new(),
                    None,
                )
                .await;
            }
            Err(_) => {
                let _ = self.connect_id(id).await;
            }
        }
    }

    async fn prepare(&self, id: &str) -> Result<(Server, HeaderMap), AppError> {
        let mut server = self.server(id).await?;
        if server.auth.kind == AuthKind::OAuth2 {
            let mut oauth = server
                .auth
                .oauth
                .clone()
                .ok_or_else(|| AppError::auth("oauth settings are missing"))?;
            let before = oauth.clone();
            let started = Instant::now();
            ensure_oauth_token(&self.http, &mut oauth).await?;
            if oauth != before {
                let elapsed = started.elapsed().as_millis() as u64;
                let grant = oauth.grant;
                server.auth.oauth = Some(oauth);
                let saved = self.store.save_auth(id, &server.auth)?;
                self.replace_auth(saved).await;
                self.record_activity(
                    Some(id.to_string()),
                    &server.slug,
                    "system",
                    "oauth",
                    "ok",
                    "oauth/token",
                    None,
                    Some(elapsed),
                    true,
                    format!("OAuth2 token renewed ({grant:?})"),
                    None,
                    None,
                )
                .await;
            }
        }
        let headers = build_auth_headers(&server.auth)?;
        Ok((server, headers))
    }

    async fn refresh_auth(&self, id: &str) -> Result<(), AppError> {
        let mut server = self.server(id).await?;
        let mut oauth = server
            .auth
            .oauth
            .clone()
            .ok_or_else(|| AppError::auth("oauth settings are missing"))?;
        let started = Instant::now();
        force_refresh(&self.http, &mut oauth).await?;
        let elapsed = started.elapsed().as_millis() as u64;
        server.auth.oauth = Some(oauth);
        let saved = self.store.save_auth(id, &server.auth)?;
        self.replace_auth(saved).await;
        self.record_activity(
            Some(id.to_string()),
            &server.slug,
            "system",
            "oauth",
            "ok",
            "oauth/refresh",
            None,
            Some(elapsed),
            true,
            "forced OAuth2 token refresh after 401/403",
            None,
            None,
        )
        .await;
        Ok(())
    }

    async fn server(&self, id: &str) -> Result<Server, AppError> {
        self.inner
            .read()
            .await
            .get(id)
            .map(|live| live.server.clone())
            .ok_or_else(|| AppError::not_found("server not found"))
    }

    async fn upstream_of(&self, id: &str) -> Option<Upstream> {
        self.inner
            .read()
            .await
            .get(id)
            .and_then(|live| live.upstream.clone())
    }

    async fn message_of(&self, id: &str) -> String {
        self.inner
            .read()
            .await
            .get(id)
            .map(|live| {
                if live.message.is_empty() {
                    "server is not connected".to_string()
                } else {
                    live.message.clone()
                }
            })
            .unwrap_or_else(|| "server is not connected".to_string())
    }

    pub async fn note_server(&self, server: Server) {
        let mut guard = self.inner.write().await;
        if let Some(live) = guard.get_mut(&server.id) {
            live.server = server;
        } else {
            let id = server.id.clone();
            guard.insert(id, LiveServer::idle(server));
        }
    }

    async fn replace_auth(&self, server: Server) {
        self.note_server(server).await;
    }

    async fn set_state(&self, id: &str, state: ConnState, message: &str) {
        let view = {
            let mut guard = self.inner.write().await;
            let Some(live) = guard.get_mut(id) else {
                return;
            };
            live.state = state;
            live.message = message.to_string();
            live.view()
        };
        let _ = self.events.send(BusEvent::Upsert { server: view });
    }

    async fn fail(&self, id: &str, message: &str) {
        let view = {
            let mut guard = self.inner.write().await;
            let Some(live) = guard.get_mut(id) else {
                return;
            };
            live.state = ConnState::Error;
            live.message = message.to_string();
            live.upstream = None;
            live.checked_at = Some(chrono_lite::rfc3339_now());
            live.next_attempt = Some(Instant::now() + Duration::from_secs(live.backoff_secs));
            live.backoff_secs = (live.backoff_secs.saturating_mul(2)).min(60);
            live.view()
        };
        let _ = self.events.send(BusEvent::Upsert { server: view });
    }

    async fn finish(
        &self,
        id: &str,
        state: ConnState,
        message: &str,
        upstream: Option<Upstream>,
        tools: Vec<Value>,
        info: Option<Value>,
    ) {
        let view = {
            let mut guard = self.inner.write().await;
            let Some(live) = guard.get_mut(id) else {
                return;
            };
            live.state = state;
            live.message = message.to_string();
            live.tools = tools;
            live.info = info;
            live.upstream = upstream;
            live.checked_at = Some(chrono_lite::rfc3339_now());
            if state == ConnState::Connected {
                live.backoff_secs = 2;
                live.next_attempt = None;
                live.last_tool_sync = Some(Instant::now());
            }
            live.view()
        };
        let _ = self.events.send(BusEvent::Upsert { server: view });
    }
}

struct Maintain {
    id: String,
    enabled: bool,
    state: ConnState,
    next_attempt: Option<Instant>,
    last_tool_sync: Option<Instant>,
}
