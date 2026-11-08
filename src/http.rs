use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{DefaultBodyLimit, Path, Query, State, WebSocketUpgrade};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::auth::{
    apply_discovery, authorization_redirect, discover, exchange_code, pkce_pair, random_state,
    register_client,
};
use crate::catalog::{Catalog, ProbeCallResult, ProbeResult};
use crate::config::{admin_authorized, bearer_from_header, Config};
use crate::error::AppError;
use crate::import::{
    list_registry, parse_and_resolve_import, ImportRequest, ImportResponse, RegistryEntry,
};
use crate::mcp::{dispatch, SessionStore};
use crate::model::{validate_server, BusEvent, OAuthGrant, ServerInput, ToolPolicy};
use crate::store::{OAuthTx, Store};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub store: std::sync::Arc<Store>,
    pub http: reqwest::Client,
    pub catalog: std::sync::Arc<Catalog>,
    pub sessions: std::sync::Arc<SessionStore>,
    pub events: broadcast::Sender<BusEvent>,
}

impl AppState {
    pub async fn bootstrap(config: Config) -> Result<Self, AppError> {
        let store = std::sync::Arc::new(Store::open(&config)?);
        let http = reqwest::Client::builder()
            .user_agent(concat!("loopback-mcp-proxy/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|err| AppError::internal(format!("http client: {err}")))?;
        let (events, _) = broadcast::channel(512);
        let catalog =
            std::sync::Arc::new(Catalog::new(store.clone(), http.clone(), events.clone()));
        catalog.load().await?;
        Ok(Self {
            config,
            store,
            http,
            catalog,
            sessions: std::sync::Arc::new(SessionStore::default()),
            events,
        })
    }
}

pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/api/meta", get(meta))
        .route("/api/servers", get(list_servers).post(create_server))
        .route("/api/probe", post(probe))
        .route("/api/probe/call", post(probe_call))
        .route("/api/registry", get(get_registry))
        .route("/api/import/parse", post(import_parse))
        .route("/api/discover", get(discover_url))
        .route(
            "/api/servers/{id}",
            get(get_server).put(update_server).delete(delete_server),
        )
        .route("/api/servers/{id}/policy", put(update_policy).post(update_policy))
        .route("/api/servers/{id}/auth/clear", post(clear_auth))
        .route("/api/servers/{id}/reconnect", post(reconnect))
        .route("/api/servers/{id}/oauth/start", post(oauth_start))
        .route("/api/servers/{id}/tools/call", post(call_tool))
        .route("/api/activity", delete(clear_activity))
        .route("/api/vault/export", post(export_vault))
        .route("/api/vault/import", post(import_vault))
        .route("/api/ws", get(ws_route))
        .route("/oauth/callback", get(oauth_callback))
        .route("/mcp", get(mcp_get).post(mcp_post).delete(mcp_delete))
        .route(
            "/mcp/{slug}",
            get(mcp_get_scoped).post(mcp_post_scoped).delete(mcp_delete),
        )
        .fallback(static_fallback)
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(app)
}

pub async fn serve(config: Config) -> Result<(), AppError> {
    config.ensure_bind_allowed()?;
    let app = AppState::bootstrap(config).await?;
    let bind = app.config.bind;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|err| AppError::internal(format!("bind {bind}: {err}")))?;
    let catalog = app.catalog.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            catalog.maintain().await;
        }
    });
    let public = app.config.public_base_url.clone();
    tracing::info!("loopback MCP proxy listening on {bind}");
    tracing::info!("  ui   {public}/");
    tracing::info!("  mcp  {public}/mcp");
    tracing::info!("  data {}", app.config.data_dir.display());
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| AppError::internal(format!("server: {err}")))?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

fn is_loopback_host(host: &str) -> bool {
    let h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    h == "localhost"
        || h == "127.0.0.1"
        || h == "::1"
        || h == "0:0:0:0:0:0:0:1"
        || h.starts_with("127.")
}

fn split_host_header(raw: &str) -> (&str, Option<u16>) {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some((ipv6, tail)) = rest.split_once(']') {
            let port = tail.strip_prefix(':').and_then(|p| p.parse::<u16>().ok());
            return (ipv6, port);
        }
    }
    if let Some((host, port_str)) = trimmed.rsplit_once(':') {
        if !host.contains(':') {
            if let Ok(port) = port_str.parse::<u16>() {
                return (host, Some(port));
            }
        }
    }
    (trimmed, None)
}

fn verify_local_origin_and_host(app: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    if let Some(fetch_site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if fetch_site.trim().eq_ignore_ascii_case("cross-site") {
            return Err(AppError::forbidden(
                "cross-site browser request blocked (CSRF guard)",
            ));
        }
    }

    let public_parsed = url::Url::parse(&app.config.public_base_url).ok();
    let public_host = public_parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .map(|s| s.to_ascii_lowercase());
    let public_port = public_parsed
        .as_ref()
        .and_then(|u| u.port_or_known_default());

    if !app.config.allow_non_loopback {
        if let Some(raw_host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
            let (host_only, _port) = split_host_header(raw_host);
            let matches_public = public_host
                .as_deref()
                .is_some_and(|ph| ph.eq_ignore_ascii_case(host_only));
            if !is_loopback_host(host_only) && !matches_public {
                return Err(AppError::forbidden(format!(
                    "non-loopback Host header `{host_only}` rejected (DNS rebinding guard)"
                )));
            }
        }
    }

    if let Some(raw_origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        let trimmed = raw_origin.trim();
        if trimmed.eq_ignore_ascii_case("null") {
            return Err(AppError::forbidden(
                "sandboxed `Origin: null` browser request blocked (CSRF guard)",
            ));
        }
        let parsed = url::Url::parse(trimmed)
            .map_err(|_| AppError::forbidden("malformed Origin header blocked (CSRF guard)"))?;
        let origin_host = parsed.host_str().unwrap_or("");
        let origin_port = parsed.port_or_known_default();
        let matches_public = public_host
            .as_deref()
            .is_some_and(|ph| ph.eq_ignore_ascii_case(origin_host))
            && (public_port.is_none() || origin_port == public_port);
        let bind_port = app.config.bind.port();
        let host_header_port = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .and_then(|h| split_host_header(h).1);
        let port_allowed = origin_port == Some(bind_port)
            || origin_port == public_port
            || (host_header_port.is_some() && origin_port == host_header_port);
        let host_allowed = is_loopback_host(origin_host)
            || matches_public
            || (app.config.allow_non_loopback
                && headers
                    .get(header::HOST)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|h| split_host_header(h).0.eq_ignore_ascii_case(origin_host)));
        if !host_allowed || !port_allowed {
            return Err(AppError::forbidden(format!(
                "cross-origin browser request from `{trimmed}` blocked (CSRF guard)"
            )));
        }
    }

    Ok(())
}

fn require_admin(app: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    verify_local_origin_and_host(app, headers)?;
    let presented = bearer_from_header(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
    );
    if admin_authorized(app.config.admin_token.as_deref(), presented) {
        Ok(())
    } else {
        Err(AppError::unauthorized())
    }
}

async fn meta(State(app): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "authRequired": app.config.admin_token.is_some(),
        "publicBaseUrl": app.config.public_base_url,
        "mcpUrl": format!("{}/mcp", app.config.public_base_url),
    }))
}

async fn list_servers(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::model::ServerView>>, AppError> {
    require_admin(&app, &headers)?;
    Ok(Json(app.catalog.views().await))
}

async fn get_server(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<crate::model::ServerView>, AppError> {
    require_admin(&app, &headers)?;
    let id = app.catalog.id_for_slug(&id).await?;
    Ok(Json(app.catalog.view(&id).await?))
}

async fn create_server(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ServerInput>,
) -> Result<impl IntoResponse, AppError> {
    require_admin(&app, &headers)?;
    crate::model::validate_target_url(
        &input.url,
        Some(app.config.bind.port()),
        Some(&app.config.public_base_url),
    )?;
    validate_server(&input, None)?;
    let id = uuid::Uuid::new_v4().to_string();
    let server = app.store.insert(&id, input)?;
    tracing::info!(slug = %server.slug, url = %server.url, "added upstream");
    app.catalog.remember(server).await;
    let view = app.catalog.connect_id(&id).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn update_server(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ServerInput>,
) -> Result<Json<crate::model::ServerView>, AppError> {
    require_admin(&app, &headers)?;
    crate::model::validate_target_url(
        &input.url,
        Some(app.config.bind.port()),
        Some(&app.config.public_base_url),
    )?;
    let current = app.store.get(&id)?;
    validate_server(&input, Some(&current))?;
    let server = app.store.update(&id, input)?;
    tracing::info!(slug = %server.slug, "updated upstream");
    app.catalog.remember(server).await;
    Ok(Json(app.catalog.connect_id(&id).await?))
}

async fn delete_server(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_admin(&app, &headers)?;
    app.catalog.remove(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_policy(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(policy): Json<ToolPolicy>,
) -> Result<Json<crate::model::ServerView>, AppError> {
    require_admin(&app, &headers)?;
    let server = app.store.save_tool_policy(&id, &policy)?;
    Ok(Json(app.catalog.update_tool_policy(server).await?))
}

async fn clear_auth(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<crate::model::ServerView>, AppError> {
    require_admin(&app, &headers)?;
    Ok(Json(app.catalog.clear_server_auth(&id).await?))
}

async fn clear_activity(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    require_admin(&app, &headers)?;
    app.catalog.clear_activity_log().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct VaultExportBody {
    passphrase: String,
}

async fn export_vault(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<VaultExportBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&app, &headers)?;
    let bundle = app.store.export_bundle(&body.passphrase)?;
    app.catalog
        .record_activity(
            None,
            "vault",
            "admin",
            "state",
            "ok",
            "vault/export",
            None,
            Some(0),
            true,
            "exported passphrase-encrypted vault bundle",
            None,
            None,
        )
        .await;
    Ok(Json(bundle))
}

#[derive(Deserialize)]
struct VaultImportBody {
    passphrase: String,
    bundle: Value,
}

async fn import_vault(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<VaultImportBody>,
) -> Result<Json<Vec<crate::model::ServerView>>, AppError> {
    require_admin(&app, &headers)?;
    let servers = app.store.import_bundle(&body.passphrase, &body.bundle)?;
    for srv in &servers {
        app.catalog.remember(srv.clone()).await;
        if srv.enabled {
            let _ = app.catalog.connect_id(&srv.id).await;
        }
    }
    app.catalog
        .record_activity(
            None,
            "vault",
            "admin",
            "state",
            "ok",
            "vault/import",
            None,
            Some(0),
            true,
            format!("imported {} server(s) from encrypted vault bundle", servers.len()),
            None,
            None,
        )
        .await;
    Ok(Json(app.catalog.views().await))
}

async fn reconnect(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<crate::model::ServerView>, AppError> {
    require_admin(&app, &headers)?;
    Ok(Json(app.catalog.connect_id(&id).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeBody {
    #[serde(flatten)]
    input: ServerInput,
    server_id: Option<String>,
}

async fn probe(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<ProbeBody>,
) -> Result<Json<ProbeResult>, AppError> {
    require_admin(&app, &headers)?;
    crate::model::validate_target_url(
        &body.input.url,
        Some(app.config.bind.port()),
        Some(&app.config.public_base_url),
    )?;
    let previous = match body.server_id.as_deref() {
        Some(id) => Some(app.store.get(id)?),
        None => None,
    };
    validate_server(&body.input, previous.as_ref())?;
    body.input.auth = crate::model::merge_server_auth(
        previous.as_ref(),
        &body.input.url,
        body.input.auth,
    );
    Ok(Json(
        app.catalog
            .probe_with_logs(&body.input, body.server_id.as_deref())
            .await,
    ))
}

async fn probe_call(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> Result<Json<ProbeCallResult>, AppError> {
    require_admin(&app, &headers)?;
    let tool_name = raw
        .get("toolName")
        .or_else(|| raw.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if tool_name.is_empty() {
        return Err(AppError::bad("toolName is required"));
    }
    let input_val = raw.get("input").cloned().unwrap_or_else(|| raw.clone());
    let mut input: ServerInput = serde_json::from_value(input_val)
        .map_err(|err| AppError::bad(format!("invalid server input: {err}")))?;
    crate::model::validate_target_url(
        &input.url,
        Some(app.config.bind.port()),
        Some(&app.config.public_base_url),
    )?;
    let server_id = raw
        .get("serverId")
        .or_else(|| raw.get("input").and_then(|i| i.get("serverId")))
        .and_then(Value::as_str)
        .map(str::to_string);
    let previous = match server_id.as_deref() {
        Some(id) => Some(app.store.get(id)?),
        None => None,
    };
    validate_server(&input, previous.as_ref())?;
    input.auth = crate::model::merge_server_auth(
        previous.as_ref(),
        &input.url,
        input.auth,
    );
    let arguments = match raw.get("arguments").cloned() {
        Some(v) if !v.is_null() => v,
        _ => json!({}),
    };
    Ok(Json(
        app.catalog
            .probe_call(&input, server_id.as_deref(), &tool_name, arguments)
            .await,
    ))
}

#[derive(Deserialize)]
struct UrlQuery {
    url: String,
}

async fn discover_url(
    State(app): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UrlQuery>,
) -> Result<Json<crate::model::Discovery>, AppError> {
    require_admin(&app, &headers)?;
    crate::model::validate_target_url(
        &query.url,
        Some(app.config.bind.port()),
        Some(&app.config.public_base_url),
    )?;
    Ok(Json(discover(&app.http, &query.url).await?))
}

async fn get_registry(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RegistryEntry>>, AppError> {
    require_admin(&app, &headers)?;
    Ok(Json(list_registry()))
}

async fn import_parse(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportResponse>, AppError> {
    require_admin(&app, &headers)?;
    let started = std::time::Instant::now();
    let res = parse_and_resolve_import(&app.http, req).await;
    let duration_ms = started.elapsed().as_millis() as u64;
    match &res {
        Ok(parsed) => {
            let first_slug = parsed
                .candidates
                .first()
                .map(|c| c.input.slug.clone())
                .unwrap_or_else(|| "import".to_string());
            let preview_json = serde_json::json!({
                "detectedFormat": parsed.detected_format,
                "sourceLabel": parsed.source_label,
                "candidateCount": parsed.candidates.len(),
                "candidates": parsed.candidates.iter().map(|c| serde_json::json!({
                    "name": c.input.name,
                    "slug": c.input.slug,
                    "url": c.input.url,
                    "transport": c.input.transport,
                    "authKind": c.input.auth.kind,
                    "notes": c.notes,
                    "warnings": c.warnings,
                })).collect::<Vec<_>>(),
            });
            app.catalog
                .record_activity(
                    None,
                    first_slug,
                    "probe",
                    "handshake",
                    "response",
                    "import/parse",
                    None,
                    Some(duration_ms),
                    true,
                    format!(
                        "Parsed {} [{}] ({} upstream candidate{})",
                        parsed.source_label,
                        parsed.detected_format,
                        parsed.candidates.len(),
                        if parsed.candidates.len() == 1 { "" } else { "s" }
                    ),
                    None,
                    Some(preview_json.to_string()),
                )
                .await;
        }
        Err(err) => {
            app.catalog
                .record_activity(
                    None,
                    "import",
                    "probe",
                    "error",
                    "error",
                    "import/parse",
                    None,
                    Some(duration_ms),
                    false,
                    format!("Import parse failed: {}", err.message),
                    None,
                    None,
                )
                .await;
        }
    }
    Ok(Json(res?))
}

async fn oauth_start(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_admin(&app, &headers)?;
    let mut server = app.store.get(&id)?;
    if server.auth.kind != crate::model::AuthKind::OAuth2 {
        return Err(AppError::bad("server does not use oauth"));
    }
    let mut oauth = server
        .auth
        .oauth
        .clone()
        .ok_or_else(|| AppError::bad("oauth settings are missing"))?;
    if oauth.grant != OAuthGrant::AuthorizationCode {
        return Err(AppError::bad(
            "client_credentials does not use a browser redirect",
        ));
    }
    if let Ok(found) = discover(&app.http, &server.url).await {
        apply_discovery(&mut oauth, &found);
    }
    let redirect_uri = format!("{}/oauth/callback", app.config.public_base_url);
    if oauth.client_id.trim().is_empty() {
        let endpoint = oauth.registration_endpoint.clone().ok_or_else(|| {
            AppError::bad("client id is empty and no registration endpoint was discovered")
        })?;
        let (client_id, secret) =
            register_client(&app.http, &endpoint, &redirect_uri, &oauth.scopes).await?;
        oauth.client_id = client_id;
        if secret.is_some() {
            oauth.client_secret = secret;
        }
    }
    if oauth.resource.as_ref().is_none_or(|value| value.is_empty()) {
        oauth.resource = Some(server.url.clone());
    }
    let (verifier, challenge) = pkce_pair();
    let state = random_state();
    let authorize_url = authorization_redirect(&oauth, &redirect_uri, &state, &challenge)?;
    server.auth.oauth = Some(oauth);
    let saved = app.store.save_auth(&id, &server.auth)?;
    app.catalog.note_server(saved).await;
    app.store.put_oauth_tx(
        &state,
        &OAuthTx {
            server_id: id,
            verifier,
            redirect_uri,
        },
    )?;
    Ok(Json(json!({ "authorizeUrl": authorize_url })))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn oauth_callback(
    State(app): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Redirect {
    let base = app.config.public_base_url.clone();
    if let Some(error) = query.error {
        let message = query.error_description.unwrap_or(error);
        return Redirect::temporary(&format!("{base}/#/?oauthError={}", encode(&message)));
    }
    let Some(state) = query.state else {
        return Redirect::temporary(&format!("{base}/#/?oauthError=missing_state"));
    };
    let Some(code) = query.code else {
        return Redirect::temporary(&format!("{base}/#/?oauthError=missing_code"));
    };
    let tx = match app.store.take_oauth_tx(&state) {
        Ok(tx) => tx,
        Err(err) => {
            return Redirect::temporary(&format!("{base}/#/?oauthError={}", encode(&err.message)));
        }
    };
    let fail = |message: String| {
        Redirect::temporary(&format!(
            "{base}/#/servers/{}?oauthError={}",
            tx.server_id,
            encode(&message)
        ))
    };
    let mut server = match app.store.get(&tx.server_id) {
        Ok(server) => server,
        Err(err) => return fail(err.message),
    };
    let mut oauth = match server.auth.oauth.clone() {
        Some(oauth) => oauth,
        None => return fail("oauth settings are missing".into()),
    };
    if let Err(err) =
        exchange_code(&app.http, &mut oauth, &code, &tx.redirect_uri, &tx.verifier).await
    {
        return fail(err.message);
    }
    server.auth.oauth = Some(oauth);
    match app.store.save_auth(&tx.server_id, &server.auth) {
        Ok(saved) => app.catalog.note_server(saved).await,
        Err(err) => return fail(err.message),
    }
    let _ = app.catalog.connect_id(&tx.server_id).await;
    Redirect::temporary(&format!("{base}/#/servers/{}", tx.server_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallBody {
    name: String,
    #[serde(default)]
    arguments: Value,
}

async fn call_tool(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ToolCallBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&app, &headers)?;
    let id = app.catalog.id_for_slug(&id).await?;
    let arguments = if body.arguments.is_null() {
        json!({})
    } else {
        body.arguments
    };
    Ok(Json(
        app.catalog.call_tool(&id, &body.name, arguments).await?,
    ))
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn ws_route(
    State(app): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let presented = bearer_from_header(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
    )
    .or(query.token.as_deref());
    if !admin_authorized(app.config.admin_token.as_deref(), presented) {
        return AppError::unauthorized().into_response();
    }
    upgrade.on_upgrade(move |socket| ws_loop(app, socket))
}

async fn ws_loop(app: AppState, mut socket: WebSocket) {
    let mut events = app.events.subscribe();
    let snapshot = app.catalog.views().await;
    let activity = app.catalog.recent_activity().await;
    let hello = json!({ "type": "snapshot", "servers": snapshot, "activity": activity });
    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let mut ping = tokio::time::interval(Duration::from_secs(20));
    loop {
        tokio::select! {
            _ = ping.tick() => {
                let _ = app.events.send(BusEvent::Ping);
            }
            incoming = socket.recv() => {
                if incoming.is_none() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let text = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

async fn mcp_post(State(app): State<AppState>, headers: HeaderMap, body: String) -> Response {
    mcp_handle(&app, None, &headers, &body).await
}

async fn mcp_post_scoped(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    body: String,
) -> Response {
    mcp_handle(&app, Some(slug.as_str()), &headers, &body).await
}

async fn mcp_get(State(app): State<AppState>, upgrade_headers: HeaderMap) -> Response {
    sse_stream(&app, None, upgrade_headers).await
}

async fn mcp_get_scoped(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    sse_stream(&app, Some(slug), headers).await
}

async fn mcp_delete(headers: HeaderMap, State(app): State<AppState>) -> Response {
    if let Err(err) = verify_local_origin_and_host(&app, &headers) {
        return (
            err.status,
            Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32003,"message":err.message}})),
        )
            .into_response();
    }
    let session = headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if app.sessions.close(session) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn mcp_handle(
    app: &AppState,
    scope: Option<&str>,
    headers: &HeaderMap,
    body: &str,
) -> Response {
    if let Err(err) = verify_local_origin_and_host(app, headers) {
        return (
            err.status,
            Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32003,"message":err.message}})),
        )
            .into_response();
    }
    let session = headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok());
    let payload = if body.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"empty body"}})),
        )
            .into_response();
    } else {
        match serde_json::from_str::<Value>(body) {
            Ok(value) => value,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}})),
                )
                    .into_response();
            }
        }
    };
    let result = dispatch(&app.catalog, &app.sessions, scope, session, payload).await;
    let mut response = match result.body {
        Some(body) => (result.status, Json(body)).into_response(),
        None => result.status.into_response(),
    };
    if let Some(session_id) = result.session_id {
        if let Ok(value) = header::HeaderValue::from_str(&session_id) {
            response.headers_mut().insert("mcp-session-id", value);
        }
    }
    response
}

async fn sse_stream(app: &AppState, scope: Option<String>, headers: HeaderMap) -> Response {
    if let Err(err) = verify_local_origin_and_host(app, &headers) {
        return (
            err.status,
            Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32003,"message":err.message}})),
        )
            .into_response();
    }
    let target_id = match scope.as_deref() {
        Some(slug) => app.catalog.id_for_slug(slug).await.ok(),
        None => None,
    };
    let stream = mcp_event_stream(app.events.clone(), scope, target_id);
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

fn mcp_event_stream(
    events: broadcast::Sender<BusEvent>,
    scope: Option<String>,
    target_id: Option<String>,
) -> impl futures_util::Stream<Item = Result<String, std::io::Error>> {
    let rx = events.subscribe();
    let ping = tokio::time::interval(Duration::from_secs(15));
    let started = false;
    futures_util::stream::unfold(
        (rx, ping, started, scope, target_id),
        move |mut state| async move {
            if !state.2 {
                state.2 = true;
                return Some((Ok(": connected\n\n".to_string()), state));
            }
            loop {
                tokio::select! {
                    _ = state.1.tick() => {
                        return Some((Ok(": ping\n\n".to_string()), state));
                    }
                    event = state.0.recv() => {
                        match event {
                            Ok(BusEvent::Upsert { server }) => {
                                let matches = match state.3.as_deref() {
                                    Some(target) => server.slug == target || server.id == target,
                                    None => true,
                                };
                                if matches {
                                    state.4 = Some(server.id);
                                    let frame = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\n\n";
                                    return Some((Ok(frame.to_string()), state));
                                }
                            }
                            Ok(BusEvent::Remove { id }) => {
                                let matches = match (&state.3, &state.4) {
                                    (None, _) => true,
                                    (Some(target), _) if target == &id => true,
                                    (_, Some(known_id)) if known_id == &id => true,
                                    _ => false,
                                };
                                if matches {
                                    let frame = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\n\n";
                                    return Some((Ok(frame.to_string()), state));
                                }
                            }
                            Ok(_) => continue,
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => return None,
                        }
                    }
                }
            }
        },
    )
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

async fn static_fallback(uri: Uri) -> Response {
    crate::web::serve(uri.path())
}
