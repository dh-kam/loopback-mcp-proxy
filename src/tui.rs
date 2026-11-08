use std::collections::HashMap;
use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::error::AppError;
use crate::model::{
    AuthConfig, AuthKind, ConnState, HeaderPair, OAuthConfig, OAuthGrant, ServerInput, ServerView,
    ToolPolicy, ToolPolicyMode, Transport,
};

const AMBER: Color = Color::Rgb(255, 176, 0);
const GREEN: Color = Color::Rgb(182, 242, 196);
const RED: Color = Color::Rgb(255, 93, 58);
const INK: Color = Color::Rgb(243, 234, 215);
const MUTED: Color = Color::Rgb(163, 152, 128);

pub async fn run(base_url: &str, token: Option<&str>) -> Result<(), AppError> {
    let mut terminal = TerminalGuard::enter()?;
    let api = Api::new(base_url, token.map(str::to_string));
    let mut model = Model::default();
    let mut events = EventStream::new();
    let mut socket: Option<Ws> = None;
    loop {
        terminal.draw(|frame| draw(frame, &model))?;
        let reconnect_after = if socket.is_none() && model.notice == "connecting" {
            Duration::from_millis(20)
        } else {
            Duration::from_secs(2)
        };
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break };
                let event = event.map_err(|err| AppError::internal(format!("terminal: {err}")))?;
                if handle_key(&mut model, event) == Step::Quit {
                    break;
                }
            }
            message = next_ws(socket.as_mut()), if socket.is_some() => {
                match message {
                    Some(Ok(text)) => apply_ws(&mut model, &text),
                    _ => {
                        socket = None;
                        model.live = false;
                        model.notice = "reconnecting".into();
                    }
                }
            }
            _ = tokio::time::sleep(reconnect_after), if socket.is_none() => {
                match tokio::time::timeout(Duration::from_secs(3), connect_ws(&api)).await {
                    Ok(Ok(ws)) => {
                        socket = Some(ws);
                        model.live = true;
                        model.notice = "live".into();
                    }
                    Ok(Err(err)) => {
                        model.live = false;
                        model.notice = format!("offline: {err}");
                    }
                    Err(_) => {
                        model.live = false;
                        model.notice = "offline: websocket timed out".into();
                    }
                }
            }
        }
        if let Some(action) = model.action.take() {
            model.notice = run_action(&api, &mut model, action).await;
        }
    }
    Ok(())
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, AppError> {
        enable_raw_mode().map_err(|err| AppError::internal(format!("raw mode: {err}")))?;
        let mut stdout = io::stdout();
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|err| AppError::internal(format!("terminal: {err}")))?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))
            .map_err(|err| AppError::internal(format!("terminal: {err}")))?;
        Ok(Self { terminal })
    }

    fn draw(&mut self, draw: impl FnOnce(&mut ratatui::Frame)) -> Result<(), AppError> {
        self.terminal
            .draw(draw)
            .map(|_| ())
            .map_err(|err| AppError::internal(format!("draw: {err}")))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect_ws(api: &Api) -> Result<Ws, AppError> {
    let mut request = api
        .ws_url()
        .into_client_request()
        .map_err(|err| AppError::internal(format!("websocket url: {err}")))?;
    if let Some(token) = &api.token {
        let value = format!("Bearer {token}")
            .parse()
            .map_err(|_| AppError::internal("admin token is not a valid header"))?;
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
            value,
        );
    }
    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|err| AppError::upstream(format!("websocket: {err}")))?;
    Ok(socket)
}

async fn next_ws(socket: Option<&mut Ws>) -> Option<Result<String, ()>> {
    let socket = socket?;
    match socket.next().await {
        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => Some(Ok(text.to_string())),
        Some(Ok(_)) => Some(Ok(String::new())),
        Some(Err(_)) | None => None,
        // The match above already returns. This arm is unreachable if None is covered.
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Stay,
    Quit,
}

enum Action {
    Save,
    Delete,
    Reconnect,
    Probe,
    ToggleShield,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Main,
    Form,
    Delete,
}

struct Model {
    servers: Vec<ServerView>,
    selected: usize,
    screen: Screen,
    form: Form,
    notice: String,
    live: bool,
    action: Option<Action>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            selected: 0,
            screen: Screen::Main,
            form: Form::create(),
            notice: "connecting".into(),
            live: false,
            action: None,
        }
    }
}

struct Form {
    editing: Option<String>,
    cursor: usize,
    slug_manual: bool,
    values: HashMap<String, String>,
}

impl Form {
    fn create() -> Self {
        let mut values = HashMap::new();
        values.insert("transport".into(), "streamable-http".into());
        values.insert("enabled".into(), "true".into());
        values.insert("authKind".into(), "none".into());
        values.insert("oauthGrant".into(), "authorization_code".into());
        Self {
            editing: None,
            cursor: 0,
            slug_manual: false,
            values,
        }
    }

    fn from_server(server: &ServerView) -> Self {
        let mut form = Self::create();
        form.editing = Some(server.id.clone());
        form.slug_manual = true;
        form.values.insert("name".into(), server.name.clone());
        form.values.insert("slug".into(), server.slug.clone());
        form.values.insert("url".into(), server.url.clone());
        form.values
            .insert("transport".into(), server.transport.as_str().into());
        form.values.insert(
            "enabled".into(),
            if server.enabled { "true" } else { "false" }.into(),
        );
        form.values.insert(
            "authKind".into(),
            match server.auth.kind {
                AuthKind::None => "none",
                AuthKind::Bearer => "bearer",
                AuthKind::Basic => "basic",
                AuthKind::Headers => "headers",
                AuthKind::OAuth2 => "oauth2",
            }
            .into(),
        );
        if let Some(user) = &server.auth.basic_username {
            form.values.insert("basicUsername".into(), user.clone());
        }
        if let Some(realm) = &server.auth.basic_realm {
            form.values.insert("basicRealm".into(), realm.clone());
        }
        if !server.auth.header_names.is_empty() {
            form.values
                .insert("headers".into(), server.auth.header_names.join("\n"));
        }
        if let Some(oauth) = &server.auth.oauth {
            form.values.insert(
                "oauthGrant".into(),
                match oauth.grant {
                    OAuthGrant::AuthorizationCode => "authorization_code",
                    OAuthGrant::ClientCredentials => "client_credentials",
                }
                .into(),
            );
            form.values
                .insert("clientId".into(), oauth.client_id.clone());
            form.values.insert(
                "authorizeUrl".into(),
                oauth.authorize_url.clone().unwrap_or_default(),
            );
            form.values
                .insert("tokenUrl".into(), oauth.token_url.clone());
            form.values.insert("scopes".into(), oauth.scopes.join(" "));
        }
        form
    }

    fn fields(&self) -> Vec<Field> {
        let mut fields = vec![
            Field::text("name", "name"),
            Field::text("slug", "slug"),
            Field::text("url", "url"),
            Field::choice("transport", "transport", &["streamable-http", "sse"]),
            Field::choice("enabled", "enabled", &["true", "false"]),
            Field::choice(
                "authKind",
                "auth",
                &["none", "bearer", "basic", "headers", "oauth2"],
            ),
        ];
        match self.values.get("authKind").map(String::as_str) {
            Some("bearer") => fields.push(Field::secret("bearerToken", "bearer token")),
            Some("basic") => {
                fields.push(Field::text("basicUsername", "username"));
                fields.push(Field::secret("basicPassword", "password"));
                fields.push(Field::text("basicRealm", "realm"));
            }
            Some("headers") => fields.push(Field::text("headers", "headers name: value")),
            Some("oauth2") => {
                fields.push(Field::choice(
                    "oauthGrant",
                    "grant",
                    &["authorization_code", "client_credentials"],
                ));
                fields.push(Field::text("clientId", "client id"));
                fields.push(Field::secret("clientSecret", "client secret"));
                fields.push(Field::text("authorizeUrl", "authorize url"));
                fields.push(Field::text("tokenUrl", "token url"));
                fields.push(Field::text("scopes", "scopes"));
                fields.push(Field::secret("accessToken", "access token"));
                fields.push(Field::secret("refreshToken", "refresh token"));
            }
            _ => {}
        }
        fields
    }
}

struct Field {
    key: &'static str,
    label: &'static str,
    kind: FieldKind,
}

impl Field {
    fn text(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Text,
        }
    }

    fn secret(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Secret,
        }
    }

    fn choice(key: &'static str, label: &'static str, options: &'static [&'static str]) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Choice(options),
        }
    }
}

enum FieldKind {
    Text,
    Secret,
    Choice(&'static [&'static str]),
}

fn handle_key(model: &mut Model, event: Event) -> Step {
    let Event::Key(key) = event else {
        return Step::Stay;
    };
    if key.kind != KeyEventKind::Press {
        return Step::Stay;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Step::Quit;
    }
    match model.screen {
        Screen::Main => match key.code {
            KeyCode::Char('q') => Step::Quit,
            KeyCode::Down | KeyCode::Char('j') => {
                if !model.servers.is_empty() {
                    model.selected = (model.selected + 1).min(model.servers.len() - 1);
                }
                Step::Stay
            }
            KeyCode::Up | KeyCode::Char('k') => {
                model.selected = model.selected.saturating_sub(1);
                Step::Stay
            }
            KeyCode::Char('a') => {
                model.form = Form::create();
                model.screen = Screen::Form;
                Step::Stay
            }
            KeyCode::Char('e') => {
                if let Some(server) = model.servers.get(model.selected) {
                    model.form = Form::from_server(server);
                    model.screen = Screen::Form;
                }
                Step::Stay
            }
            KeyCode::Char('d') => {
                if !model.servers.is_empty() {
                    model.screen = Screen::Delete;
                }
                Step::Stay
            }
            KeyCode::Char('r') => {
                model.action = Some(Action::Reconnect);
                Step::Stay
            }
            KeyCode::Char('p') => {
                model.action = Some(Action::ToggleShield);
                Step::Stay
            }
            _ => Step::Stay,
        },
        Screen::Delete => match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                model.action = Some(Action::Delete);
                model.screen = Screen::Main;
                Step::Stay
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                model.screen = Screen::Main;
                Step::Stay
            }
            _ => Step::Stay,
        },
        Screen::Form => edit_form(model, key.code, key.modifiers),
    }
}

fn edit_form(model: &mut Model, code: KeyCode, modifiers: KeyModifiers) -> Step {
    let fields = model.form.fields();
    if fields.is_empty() {
        return Step::Stay;
    }
    model.form.cursor = model.form.cursor.min(fields.len() - 1);
    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('s')) {
        model.action = Some(Action::Save);
        return Step::Stay;
    }
    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('t')) {
        model.action = Some(Action::Probe);
        return Step::Stay;
    }
    match code {
        KeyCode::Esc => {
            model.screen = Screen::Main;
            Step::Stay
        }
        KeyCode::Tab | KeyCode::Down => {
            model.form.cursor = (model.form.cursor + 1).min(fields.len() - 1);
            Step::Stay
        }
        KeyCode::BackTab | KeyCode::Up => {
            model.form.cursor = model.form.cursor.saturating_sub(1);
            Step::Stay
        }
        KeyCode::Left | KeyCode::Right => {
            let field = &fields[model.form.cursor];
            if let FieldKind::Choice(options) = field.kind {
                let current = model
                    .form
                    .values
                    .get(field.key)
                    .cloned()
                    .unwrap_or_default();
                let index = options
                    .iter()
                    .position(|option| *option == current)
                    .unwrap_or(0);
                let next = match code {
                    KeyCode::Right => (index + 1) % options.len(),
                    _ => (index + options.len() - 1) % options.len(),
                };
                model
                    .form
                    .values
                    .insert(field.key.into(), options[next].into());
            }
            Step::Stay
        }
        KeyCode::Backspace => {
            let field = &fields[model.form.cursor];
            if !matches!(field.kind, FieldKind::Choice(_)) {
                let mut updated = model
                    .form
                    .values
                    .get(field.key)
                    .cloned()
                    .unwrap_or_default();
                updated.pop();
                if field.key == "slug" {
                    model.form.slug_manual = true;
                }
                model.form.values.insert(field.key.into(), updated);
            }
            Step::Stay
        }
        KeyCode::Char(ch) => {
            let field = &fields[model.form.cursor];
            if !matches!(field.kind, FieldKind::Choice(_)) {
                let mut updated = model
                    .form
                    .values
                    .get(field.key)
                    .cloned()
                    .unwrap_or_default();
                updated.push(ch);
                if field.key == "name" && !model.form.slug_manual {
                    let slug = slugify(&updated);
                    model.form.values.insert("slug".into(), slug);
                }
                if field.key == "slug" {
                    model.form.slug_manual = true;
                }
                model.form.values.insert(field.key.into(), updated);
            }
            Step::Stay
        }
        _ => Step::Stay,
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            hyphen = false;
        } else if !out.is_empty() && !hyphen {
            out.push('-');
            hyphen = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn apply_ws(model: &mut Model, text: &str) {
    if text.is_empty() {
        return;
    }
    let Ok(message) = serde_json::from_str::<WsMessage>(text) else {
        return;
    };
    let selected = model
        .servers
        .get(model.selected)
        .map(|server| server.id.clone());
    match message {
        WsMessage::Snapshot { servers } => model.servers = servers,
        WsMessage::Upsert { server } => {
            if let Some(slot) = model.servers.iter_mut().find(|item| item.id == server.id) {
                *slot = server;
            } else {
                model.servers.push(server);
            }
        }
        WsMessage::Remove { id } => model.servers.retain(|server| server.id != id),
        WsMessage::Activity { .. } | WsMessage::Ping => return,
    }
    model
        .servers
        .sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.name.cmp(&b.name)));
    if let Some(selected) = selected {
        if let Some(index) = model
            .servers
            .iter()
            .position(|server| server.id == selected)
        {
            model.selected = index;
        }
    }
    if model.selected >= model.servers.len() {
        model.selected = model.servers.len().saturating_sub(1);
    }
}

async fn run_action(api: &Api, model: &mut Model, action: Action) -> String {
    match action {
        Action::Save => match form_input(&model.form) {
            Ok(input) => match api.save(model.form.editing.as_deref(), &input).await {
                Ok(server) => {
                    let id = server.id.clone();
                    if let Some(slot) = model.servers.iter_mut().find(|item| item.id == id) {
                        *slot = server;
                    } else {
                        model.servers.push(server);
                    }
                    model.screen = Screen::Main;
                    if let Some(index) = model.servers.iter().position(|item| item.id == id) {
                        model.selected = index;
                    }
                    let state = model
                        .servers
                        .get(model.selected)
                        .map(|server| format!("{:?}", server.state))
                        .unwrap_or_default();
                    format!("saved ({state})")
                }
                Err(err) => err,
            },
            Err(err) => err,
        },
        Action::Probe => match form_input(&model.form) {
            Ok(input) => match api.probe(model.form.editing.as_deref(), &input).await {
                Ok(result) => result,
                Err(err) => err,
            },
            Err(err) => err,
        },
        Action::Delete => {
            let Some(server) = model.servers.get(model.selected) else {
                return "nothing selected".into();
            };
            let id = server.id.clone();
            match api.delete(&id).await {
                Ok(()) => {
                    model.servers.retain(|server| server.id != id);
                    if model.selected >= model.servers.len() {
                        model.selected = model.servers.len().saturating_sub(1);
                    }
                    "deleted".into()
                }
                Err(err) => err,
            }
        }
        Action::Reconnect => {
            let Some(server) = model.servers.get(model.selected) else {
                return "nothing selected".into();
            };
            match api.reconnect(&server.id).await {
                Ok(view) => format!("{}: {}", view.slug, state_label(view.state)),
                Err(err) => err,
            }
        }
        Action::ToggleShield => {
            let Some(server) = model.servers.get(model.selected) else {
                return "nothing selected".into();
            };
            let next_mode = if server.tool_policy.mode == ToolPolicyMode::ReadOnly {
                ToolPolicyMode::AllowAll
            } else {
                ToolPolicyMode::ReadOnly
            };
            let next_policy = ToolPolicy {
                mode: next_mode,
                allowed_tools: server.tool_policy.allowed_tools.clone(),
                denied_tools: server.tool_policy.denied_tools.clone(),
            };
            match api.update_policy(&server.id, &next_policy).await {
                Ok(view) => format!(
                    "{}: policy {:?} ({}/{} tools exposed)",
                    view.slug, view.tool_policy.mode, view.exposed_tool_count, view.tool_count
                ),
                Err(err) => err,
            }
        }
    }
}

fn form_input(form: &Form) -> Result<ServerInput, String> {
    let value = |key: &str| form.values.get(key).cloned().unwrap_or_default();
    let secret = |key: &str| {
        let raw = value(key);
        if raw.is_empty() {
            None
        } else {
            Some(raw)
        }
    };
    let kind = match value("authKind").as_str() {
        "bearer" => AuthKind::Bearer,
        "basic" => AuthKind::Basic,
        "headers" => AuthKind::Headers,
        "oauth2" => AuthKind::OAuth2,
        _ => AuthKind::None,
    };
    let headers = value("headers")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (name, value) = line.split_once(':').unwrap_or((line, ""));
            HeaderPair {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            }
        })
        .collect();
    let oauth = if kind == AuthKind::OAuth2 {
        Some(OAuthConfig {
            grant: if value("oauthGrant") == "client_credentials" {
                OAuthGrant::ClientCredentials
            } else {
                OAuthGrant::AuthorizationCode
            },
            client_id: value("clientId"),
            client_secret: secret("clientSecret"),
            authorize_url: secret("authorizeUrl"),
            token_url: value("tokenUrl"),
            scopes: value("scopes")
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter(|scope| !scope.is_empty())
                .map(str::to_string)
                .collect(),
            access_token: secret("accessToken"),
            refresh_token: secret("refreshToken"),
            expires_at: None,
            resource: None,
            registration_endpoint: None,
        })
    } else {
        None
    };
    Ok(ServerInput {
        name: value("name"),
        slug: value("slug"),
        url: value("url"),
        transport: if value("transport") == "sse" {
            Transport::Sse
        } else {
            Transport::StreamableHttp
        },
        enabled: value("enabled") != "false",
        auth: AuthConfig {
            kind,
            bearer_token: secret("bearerToken"),
            basic_username: secret("basicUsername"),
            basic_password: secret("basicPassword"),
            basic_realm: secret("basicRealm"),
            headers,
            oauth,
        },
        tool_policy: None,
    })
}

fn draw(frame: &mut ratatui::Frame, model: &Model) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(frame.area());
    match model.screen {
        Screen::Form => draw_form(frame, chunks[0], model),
        Screen::Main | Screen::Delete => draw_main(frame, chunks[0], model),
    }
    let help = match model.screen {
        Screen::Main => "a add   e edit   p shield   d delete   r reconnect   q quit",
        Screen::Form => "tab next   left/right choice   ctrl-t test   ctrl-s save   esc back",
        Screen::Delete => "y delete   n cancel",
    };
    let live = if model.live { "LIVE" } else { "OFFLINE" };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            live,
            Style::default().fg(if model.live { GREEN } else { RED }),
        ),
        Span::raw("  "),
        Span::styled(help, Style::default().fg(MUTED)),
        Span::raw("  "),
        Span::styled(model.notice.as_str(), Style::default().fg(AMBER)),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" loopback "));
    frame.render_widget(footer, chunks[1]);
}

fn draw_main(frame: &mut ratatui::Frame, area: Rect, model: &Model) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    let items: Vec<ListItem> = model
        .servers
        .iter()
        .map(|server| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    lamp(server.state),
                    Style::default().fg(lamp_color(server.state)),
                ),
                Span::raw(" "),
                Span::styled(
                    server.slug.clone(),
                    Style::default().fg(INK).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", server.name), Style::default().fg(MUTED)),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !model.servers.is_empty() {
        state.select(Some(model.selected));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" servers ")
                .style(Style::default().fg(AMBER)),
        )
        .highlight_style(Style::default().bg(Color::Rgb(48, 40, 24)).fg(AMBER));
    frame.render_stateful_widget(list, cols[0], &mut state);
    let body = if let Some(server) = model.servers.get(model.selected) {
        let mut lines = vec![
            Line::from(Span::styled(
                server.name.clone(),
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("{}  {}", server.transport.as_str(), server.url)),
            Line::from(format!(
                "state {}   tools {}/{}   policy {:?}   calls {} (err {})",
                state_label(server.state),
                server.exposed_tool_count,
                server.tool_count,
                server.tool_policy.mode,
                server.total_calls,
                server.error_count
            )),
            Line::from(server.message.clone()),
            Line::from(""),
        ];
        for tool in server.tools.iter().take(12) {
            let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            lines.push(Line::from(Span::styled(
                name.to_string(),
                Style::default().fg(GREEN),
            )));
            if !description.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  {description}"),
                    Style::default().fg(MUTED),
                )));
            }
        }
        if model.screen == Screen::Delete {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Delete this server? y / n",
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            )));
        }
        lines
    } else {
        vec![Line::from("No upstreams. Press a to add one.")]
    };
    frame.render_widget(
        Paragraph::new(body).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" detail ")
                .style(Style::default().fg(AMBER)),
        ),
        cols[1],
    );
}

fn draw_form(frame: &mut ratatui::Frame, area: Rect, model: &Model) {
    let fields = model.form.fields();
    let title = if model.form.editing.is_some() {
        " edit upstream "
    } else {
        " add upstream "
    };
    let lines: Vec<Line> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let raw = model
                .form
                .values
                .get(field.key)
                .cloned()
                .unwrap_or_default();
            let shown = match field.kind {
                FieldKind::Secret if !raw.is_empty() => "*".repeat(raw.chars().count().min(24)),
                FieldKind::Secret => "(unchanged if blank)".into(),
                FieldKind::Choice(_) => format!("< {raw} >"),
                FieldKind::Text if raw.is_empty() => " ".into(),
                FieldKind::Text => raw,
            };
            let active = index == model.form.cursor;
            Line::from(vec![
                Span::styled(
                    format!("{:>16} ", field.label),
                    Style::default().fg(if active { AMBER } else { MUTED }),
                ),
                Span::styled(shown, Style::default().fg(if active { INK } else { MUTED })),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().fg(AMBER)),
        ),
        area,
    );
}

fn lamp(state: ConnState) -> &'static str {
    match state {
        ConnState::Connected => "●",
        ConnState::Connecting => "◌",
        ConnState::AuthRequired => "◑",
        ConnState::Error => "●",
        ConnState::Disabled => "○",
        ConnState::Idle => "○",
    }
}

fn lamp_color(state: ConnState) -> Color {
    match state {
        ConnState::Connected => GREEN,
        ConnState::Connecting | ConnState::Idle => AMBER,
        ConnState::AuthRequired | ConnState::Error => RED,
        ConnState::Disabled => MUTED,
    }
}

fn state_label(state: ConnState) -> &'static str {
    match state {
        ConnState::Idle => "idle",
        ConnState::Disabled => "disabled",
        ConnState::Connecting => "connecting",
        ConnState::Connected => "connected",
        ConnState::AuthRequired => "auth required",
        ConnState::Error => "error",
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WsMessage {
    Snapshot { servers: Vec<ServerView> },
    Upsert { server: ServerView },
    Remove { id: String },
    Activity {
        #[allow(dead_code)]
        entry: crate::model::ActivityEntry,
    },
    Ping,
}

struct Api {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl Api {
    fn new(base: &str, token: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            token,
        }
    }

    fn ws_url(&self) -> String {
        if let Some(rest) = self.base.strip_prefix("https://") {
            format!("wss://{rest}/api/ws")
        } else if let Some(rest) = self.base.strip_prefix("http://") {
            format!("ws://{rest}/api/ws")
        } else {
            format!("ws://{}/api/ws", self.base)
        }
    }

    async fn save(&self, id: Option<&str>, input: &ServerInput) -> Result<ServerView, String> {
        let (method, path) = match id {
            Some(id) => (reqwest::Method::PUT, format!("/api/servers/{id}")),
            None => (reqwest::Method::POST, "/api/servers".into()),
        };
        self.send(method, &path, Some(input)).await
    }

    async fn probe(&self, id: Option<&str>, input: &ServerInput) -> Result<String, String> {
        let mut body = serde_json::to_value(input).map_err(|err| err.to_string())?;
        if let Some(id) = id {
            body["serverId"] = json!(id);
        }
        let value: Value = self
            .send_value(reqwest::Method::POST, "/api/probe", Some(body))
            .await?;
        let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("probe finished");
        Ok(format!("{} {message}", if ok { "ok" } else { "failed" }))
    }

    async fn delete(&self, id: &str) -> Result<(), String> {
        self.send_status(reqwest::Method::DELETE, &format!("/api/servers/{id}"))
            .await
    }

    async fn reconnect(&self, id: &str) -> Result<ServerView, String> {
        self.send(
            reqwest::Method::POST,
            &format!("/api/servers/{id}/reconnect"),
            None::<&ServerInput>,
        )
        .await
    }

    async fn update_policy(&self, id: &str, policy: &ToolPolicy) -> Result<ServerView, String> {
        let value = serde_json::to_value(policy).map_err(|err| err.to_string())?;
        self.send_value(
            reqwest::Method::PUT,
            &format!("/api/servers/{id}/policy"),
            Some(value),
        )
        .await
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&ServerInput>,
    ) -> Result<T, String> {
        let value = body
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| err.to_string())?;
        self.send_value(method, path, value).await
    }

    async fn send_value<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, String> {
        let text = self.dispatch(method, path, body).await?;
        serde_json::from_str(&text).map_err(|err| format!("{err}: {text}"))
    }

    async fn send_status(&self, method: reqwest::Method, path: &str) -> Result<(), String> {
        self.dispatch(method, path, None).await.map(|_| ())
    }

    async fn dispatch(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<String, String> {
        let mut request = self.http.request(method, format!("{}{path}", self.base));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|err| err.to_string())?;
        let status = response.status();
        let text = response.text().await.map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("{status}: {text}"));
        }
        Ok(text)
    }
}
