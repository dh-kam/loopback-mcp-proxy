use std::fs;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::config::Config;
use crate::crypto::Crypto;
use crate::error::AppError;
use crate::model::{
    chrono_lite, ActivityEntry, AuthConfig, OAuthGrant, Server, ServerInput, ToolPolicy, Transport,
};

const OAUTH_TX_TTL_SECS: i64 = 900;

pub struct Store {
    conn: Mutex<Connection>,
    crypto: Crypto,
}

#[derive(Debug, Clone)]
pub struct OAuthTx {
    pub server_id: String,
    pub verifier: String,
    pub redirect_uri: String,
}

impl Store {
    pub fn open(config: &Config) -> Result<Self, AppError> {
        fs::create_dir_all(&config.data_dir)
            .map_err(|err| AppError::internal(format!("create data dir: {err}")))?;
        let db_path = config.data_dir.join("proxy.db");
        let conn = Connection::open(&db_path)
            .map_err(|err| AppError::internal(format!("open database: {err}")))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                url TEXT NOT NULL,
                transport TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                auth_blob TEXT NOT NULL,
                tool_policy_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS oauth_tx (
                state TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                verifier TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                created_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS activity_log (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                server_id TEXT,
                slug TEXT NOT NULL,
                source TEXT NOT NULL,
                kind TEXT NOT NULL,
                phase TEXT NOT NULL,
                method TEXT NOT NULL,
                tool TEXT,
                duration_ms INTEGER,
                ok INTEGER NOT NULL,
                message TEXT NOT NULL,
                request_preview TEXT,
                response_preview TEXT
            );",
        )
        .map_err(|err| AppError::internal(format!("migrate: {err}")))?;
        let _ = conn.execute(
            "ALTER TABLE servers ADD COLUMN tool_policy_json TEXT NOT NULL DEFAULT '{}'",
            [],
        );
        restrict_private(&db_path);
        let crypto = Crypto::load_or_create(
            &config.data_dir.join("master.key"),
            std::env::var("PROXY_DATA_KEY").ok().as_deref(),
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            crypto,
        })
    }

    pub fn list(&self) -> Result<Vec<Server>, AppError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, slug, url, transport, enabled, auth_blob, tool_policy_json, created_at, updated_at
                 FROM servers ORDER BY created_at, name",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })
            .map_err(sql_err)?;
        let mut servers = Vec::new();
        for row in rows {
            let row = row.map_err(sql_err)?;
            servers.push(self.hydrate(
                row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9,
            )?);
        }
        Ok(servers)
    }

    pub fn get(&self, id: &str) -> Result<Server, AppError> {
        self.list()?
            .into_iter()
            .find(|server| server.id == id)
            .ok_or_else(|| AppError::not_found("server not found"))
    }

    pub fn insert(&self, id: &str, input: ServerInput) -> Result<Server, AppError> {
        let now = chrono_lite::rfc3339_now();
        let blob = self.seal(&input.auth)?;
        let policy = input.tool_policy.unwrap_or_default();
        let policy_json = serde_json::to_string(&policy)
            .map_err(|err| AppError::internal(format!("encode tool policy: {err}")))?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO servers (id, name, slug, url, transport, enabled, auth_blob, tool_policy_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                input.name.trim(),
                input.slug.trim(),
                input.url.trim(),
                input.transport.as_str(),
                if input.enabled { 1 } else { 0 },
                blob,
                policy_json,
                now,
                now,
            ],
        )
        .map_err(map_sql)?;
        drop(conn);
        self.get(id)
    }

    pub fn update(&self, id: &str, input: ServerInput) -> Result<Server, AppError> {
        let current = self.get(id)?;
        let auth = crate::model::merge_server_auth(Some(&current), &input.url, input.auth);
        let blob = self.seal(&auth)?;
        let policy = input.tool_policy.unwrap_or(current.tool_policy);
        let policy_json = serde_json::to_string(&policy)
            .map_err(|err| AppError::internal(format!("encode tool policy: {err}")))?;
        let now = chrono_lite::rfc3339_now();
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE servers
                 SET name = ?1, slug = ?2, url = ?3, transport = ?4, enabled = ?5, auth_blob = ?6, tool_policy_json = ?7, updated_at = ?8
                 WHERE id = ?9",
                params![
                    input.name.trim(),
                    input.slug.trim(),
                    input.url.trim(),
                    input.transport.as_str(),
                    if input.enabled { 1 } else { 0 },
                    blob,
                    policy_json,
                    now,
                    id,
                ],
            )
            .map_err(map_sql)?;
        if changed == 0 {
            return Err(AppError::not_found("server not found"));
        }
        drop(conn);
        self.get(id)
    }

    pub fn save_tool_policy(&self, id: &str, policy: &ToolPolicy) -> Result<Server, AppError> {
        let policy_json = serde_json::to_string(policy)
            .map_err(|err| AppError::internal(format!("encode tool policy: {err}")))?;
        let now = chrono_lite::rfc3339_now();
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE servers SET tool_policy_json = ?1, updated_at = ?2 WHERE id = ?3",
                params![policy_json, now, id],
            )
            .map_err(sql_err)?;
        if changed == 0 {
            return Err(AppError::not_found("server not found"));
        }
        drop(conn);
        self.get(id)
    }

    pub fn save_auth(&self, id: &str, auth: &AuthConfig) -> Result<Server, AppError> {
        let blob = self.seal(auth)?;
        let now = chrono_lite::rfc3339_now();
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE servers SET auth_blob = ?1, updated_at = ?2 WHERE id = ?3",
                params![blob, now, id],
            )
            .map_err(sql_err)?;
        if changed == 0 {
            return Err(AppError::not_found("server not found"));
        }
        drop(conn);
        self.get(id)
    }

    pub fn clear_auth_tokens(&self, id: &str) -> Result<Server, AppError> {
        let mut server = self.get(id)?;
        server.auth.bearer_token = None;
        server.auth.basic_password = None;
        server.auth.headers.clear();
        if let Some(oauth) = server.auth.oauth.as_mut() {
            oauth.access_token = None;
            oauth.refresh_token = None;
            oauth.expires_at = None;
        }
        self.save_auth(id, &server.auth)
    }

    pub fn append_activity(&self, entry: &ActivityEntry) -> Result<u64, AppError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO activity_log (
                timestamp, server_id, slug, source, kind, phase, method, tool, duration_ms, ok, message, request_preview, response_preview
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                entry.timestamp,
                entry.server_id,
                entry.slug,
                entry.source,
                entry.kind,
                entry.phase,
                entry.method,
                entry.tool,
                entry.duration_ms.map(|d| d as i64),
                if entry.ok { 1 } else { 0 },
                entry.message,
                entry.request_preview,
                entry.response_preview,
            ],
        )
        .map_err(sql_err)?;
        let seq = conn.last_insert_rowid() as u64;
        if seq.is_multiple_of(50) {
            let _ = conn.execute(
                "DELETE FROM activity_log WHERE seq <= (SELECT MAX(seq) - 500 FROM activity_log)",
                [],
            );
        }
        Ok(seq)
    }

    pub fn load_recent_activity(&self, limit: usize) -> Result<Vec<ActivityEntry>, AppError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT seq, timestamp, server_id, slug, source, kind, phase, method, tool, duration_ms, ok, message, request_preview, response_preview
                 FROM (
                     SELECT seq, timestamp, server_id, slug, source, kind, phase, method, tool, duration_ms, ok, message, request_preview, response_preview
                     FROM activity_log
                     ORDER BY seq DESC
                     LIMIT ?1
                 )
                 ORDER BY seq ASC",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(ActivityEntry {
                    seq: row.get::<_, i64>(0)? as u64,
                    timestamp: row.get(1)?,
                    server_id: row.get(2)?,
                    slug: row.get(3)?,
                    source: row.get(4)?,
                    kind: row.get(5)?,
                    phase: row.get(6)?,
                    method: row.get(7)?,
                    tool: row.get(8)?,
                    duration_ms: row.get::<_, Option<i64>>(9)?.map(|d| d as u64),
                    ok: row.get::<_, i64>(10)? != 0,
                    message: row.get(11)?,
                    request_preview: row.get(12)?,
                    response_preview: row.get(13)?,
                })
            })
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql_err)?);
        }
        Ok(out)
    }

    pub fn clear_activity(&self) -> Result<(), AppError> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM activity_log", [])
            .map_err(sql_err)?;
        Ok(())
    }

    pub fn export_bundle(&self, passphrase: &str) -> Result<serde_json::Value, AppError> {
        let servers = self.list()?;
        let (cipher, salt_b64) = Crypto::from_passphrase(passphrase, None)?;
        let plain = serde_json::to_string(&servers)
            .map_err(|err| AppError::internal(format!("encode vault bundle: {err}")))?;
        let ciphertext = cipher.encrypt(&plain)?;
        Ok(serde_json::json!({
            "format": "loopback-mcp-vault-v1",
            "exportedAt": chrono_lite::rfc3339_now(),
            "serverCount": servers.len(),
            "salt": salt_b64,
            "ciphertext": ciphertext,
        }))
    }

    pub fn import_bundle(
        &self,
        passphrase: &str,
        bundle: &serde_json::Value,
    ) -> Result<Vec<Server>, AppError> {
        let fmt = bundle
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if fmt != "loopback-mcp-vault-v1" {
            return Err(AppError::bad(
                "unsupported vault bundle format (expected `loopback-mcp-vault-v1`)",
            ));
        }
        let salt = bundle
            .get("salt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::bad("vault bundle is missing `salt`"))?;
        let ciphertext = bundle
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::bad("vault bundle is missing `ciphertext`"))?;
        let (cipher, _) = Crypto::from_passphrase(passphrase, Some(salt))?;
        let plain = cipher.decrypt(ciphertext).map_err(|_| {
            AppError::bad("could not decrypt vault bundle (incorrect passphrase or corrupted file)")
        })?;
        let imported: Vec<Server> = serde_json::from_str(&plain)
            .map_err(|err| AppError::bad(format!("invalid vault bundle payload: {err}")))?;
        let now = chrono_lite::rfc3339_now();
        {
            let conn = self.lock()?;
            for srv in &imported {
                let blob = self.seal(&srv.auth)?;
                let policy_json =
                    serde_json::to_string(&srv.tool_policy).unwrap_or_else(|_| "{}".into());
                conn.execute(
                    "INSERT INTO servers (id, name, slug, url, transport, enabled, auth_blob, tool_policy_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(slug) DO UPDATE SET
                        name = excluded.name,
                        url = excluded.url,
                        transport = excluded.transport,
                        enabled = excluded.enabled,
                        auth_blob = excluded.auth_blob,
                        tool_policy_json = excluded.tool_policy_json,
                        updated_at = excluded.updated_at",
                    params![
                        srv.id,
                        srv.name,
                        srv.slug,
                        srv.url,
                        srv.transport.as_str(),
                        if srv.enabled { 1 } else { 0 },
                        blob,
                        policy_json,
                        srv.created_at,
                        now,
                    ],
                )
                .map_err(sql_err)?;
            }
        }
        self.list()
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let conn = self.lock()?;
        let changed = conn
            .execute("DELETE FROM servers WHERE id = ?1", params![id])
            .map_err(sql_err)?;
        if changed == 0 {
            return Err(AppError::not_found("server not found"));
        }
        let _ = conn.execute("DELETE FROM oauth_tx WHERE server_id = ?1", params![id]);
        Ok(())
    }

    pub fn put_oauth_tx(&self, state: &str, tx: &OAuthTx) -> Result<(), AppError> {
        let now = chrono_lite::unix_now();
        let conn = self.lock()?;
        let _ = conn.execute(
            "DELETE FROM oauth_tx WHERE created_unix < ?1",
            params![now.saturating_sub(OAUTH_TX_TTL_SECS)],
        );
        conn.execute(
            "INSERT OR REPLACE INTO oauth_tx (state, server_id, verifier, redirect_uri, created_unix)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                state,
                tx.server_id,
                tx.verifier,
                tx.redirect_uri,
                now,
            ],
        )
        .map_err(sql_err)?;
        Ok(())
    }

    pub fn take_oauth_tx(&self, state: &str) -> Result<OAuthTx, AppError> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT server_id, verifier, redirect_uri, created_unix FROM oauth_tx WHERE state = ?1",
                params![state],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(|_| AppError::bad("oauth state is unknown or expired"))?;
        conn.execute("DELETE FROM oauth_tx WHERE state = ?1", params![state])
            .map_err(sql_err)?;
        if chrono_lite::unix_now().saturating_sub(row.3) > OAUTH_TX_TTL_SECS {
            return Err(AppError::bad("oauth state expired"));
        }
        Ok(OAuthTx {
            server_id: row.0,
            verifier: row.1,
            redirect_uri: row.2,
        })
    }

    fn seal(&self, auth: &AuthConfig) -> Result<String, AppError> {
        let json = serde_json::to_string(auth)
            .map_err(|err| AppError::internal(format!("encode auth: {err}")))?;
        self.crypto.encrypt(&json)
    }

    #[allow(clippy::too_many_arguments)]
    fn hydrate(
        &self,
        id: String,
        name: String,
        slug: String,
        url: String,
        transport: String,
        enabled: i64,
        blob: String,
        tool_policy_json: String,
        created_at: String,
        updated_at: String,
    ) -> Result<Server, AppError> {
        let json = self.crypto.decrypt(&blob)?;
        let auth: AuthConfig = serde_json::from_str(&json)
            .map_err(|err| AppError::internal(format!("decode auth for {slug}: {err}")))?;
        let tool_policy: ToolPolicy = serde_json::from_str(&tool_policy_json).unwrap_or_default();
        Ok(Server {
            id,
            name,
            slug,
            url,
            transport: Transport::parse(&transport)?,
            enabled: enabled != 0,
            auth,
            tool_policy,
            created_at,
            updated_at,
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|_| AppError::internal("database lock poisoned"))
    }
}

fn sql_err(err: rusqlite::Error) -> AppError {
    AppError::internal(format!("database: {err}"))
}

fn map_sql(err: rusqlite::Error) -> AppError {
    if let rusqlite::Error::SqliteFailure(code, _) = &err {
        if code.code == rusqlite::ErrorCode::ConstraintViolation {
            return AppError::conflict("slug already exists");
        }
    }
    sql_err(err)
}

fn restrict_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

pub fn blank_oauth(grant: OAuthGrant) -> crate::model::OAuthConfig {
    crate::model::OAuthConfig {
        grant,
        client_id: String::new(),
        client_secret: None,
        authorize_url: None,
        token_url: String::new(),
        scopes: Vec::new(),
        access_token: None,
        refresh_token: None,
        expires_at: None,
        resource: None,
        registration_endpoint: None,
    }
}
