use rusqlite::Connection;

pub fn init_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS providers (

            id          TEXT    NOT NULL,
            app_type    TEXT    NOT NULL,
            name        TEXT    NOT NULL,
            settings_config TEXT NOT NULL DEFAULT '{}',
            website_url TEXT,
            category    TEXT,
            created_at  INTEGER,
            sort_index  INTEGER,
            notes       TEXT,
            icon        TEXT,
            icon_color  TEXT,
            meta        TEXT    NOT NULL DEFAULT '{}',
            is_current  INTEGER NOT NULL DEFAULT 0,
            in_failover_queue INTEGER NOT NULL DEFAULT 0,
            cost_multiplier TEXT NOT NULL DEFAULT '1.0',
            limit_daily_usd TEXT,
            limit_monthly_usd TEXT,
            provider_type TEXT,
            PRIMARY KEY (id, app_type)
        );

        CREATE TABLE IF NOT EXISTS provider_endpoints (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id TEXT    NOT NULL,
            app_type    TEXT    NOT NULL,
            url         TEXT    NOT NULL,
            added_at    INTEGER,
            FOREIGN KEY (provider_id, app_type) REFERENCES providers(id, app_type) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mcp_servers (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            server_config   TEXT NOT NULL DEFAULT '{}',
            description     TEXT,
            homepage        TEXT,
            docs            TEXT,
            tags            TEXT NOT NULL DEFAULT '[]',
            enabled_claude  INTEGER NOT NULL DEFAULT 0,
            enabled_codex   INTEGER NOT NULL DEFAULT 0,
            enabled_gemini  INTEGER NOT NULL DEFAULT 0,
            enabled_opencode INTEGER NOT NULL DEFAULT 0,
            enabled_hermes  INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER,
            sort_index      INTEGER
        );

        CREATE TABLE IF NOT EXISTS prompts (
            id          TEXT    NOT NULL,
            app_type    TEXT    NOT NULL,
            name        TEXT    NOT NULL,
            content     TEXT    NOT NULL DEFAULT '',
            description TEXT,
            enabled     INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER,
            updated_at  INTEGER,
            sort_index  INTEGER,
            PRIMARY KEY (id, app_type)
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS usage_logs (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            app_type        TEXT    NOT NULL,
            provider_id     TEXT    NOT NULL,
            model_id        TEXT    NOT NULL DEFAULT '',
            input_tokens    INTEGER NOT NULL DEFAULT 0,
            output_tokens   INTEGER NOT NULL DEFAULT 0,
            cost_usd        REAL    NOT NULL DEFAULT 0.0,
            timestamp       INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_usage_logs_app_ts ON usage_logs(app_type, timestamp);
        CREATE INDEX IF NOT EXISTS idx_usage_logs_provider ON usage_logs(provider_id, timestamp);

        CREATE TABLE IF NOT EXISTS skills (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            description     TEXT,
            directory       TEXT,
            repo_owner      TEXT,
            repo_name       TEXT,
            repo_branch     TEXT DEFAULT 'main',
            readme_url      TEXT,
            enabled_claude  INTEGER NOT NULL DEFAULT 0,
            enabled_codex   INTEGER NOT NULL DEFAULT 0,
            enabled_gemini  INTEGER NOT NULL DEFAULT 0,
            enabled_opencode INTEGER NOT NULL DEFAULT 0,
            enabled_hermes  INTEGER NOT NULL DEFAULT 0,
            installed_at    INTEGER NOT NULL DEFAULT 0,
            content_hash    TEXT,
            updated_at      INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER,
            sort_index      INTEGER
        );
        ",
    )?;

    // Migrations for columns added after initial release.
    // ALTER TABLE … ADD COLUMN fails with "duplicate column" on DBs that already
    // have the column, so we silently ignore those errors.
    let migrations = [
        "ALTER TABLE mcp_servers ADD COLUMN docs TEXT",
        "ALTER TABLE mcp_servers ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE mcp_servers ADD COLUMN enabled_hermes INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE providers ADD COLUMN cost_multiplier TEXT NOT NULL DEFAULT '1.0'",
        "ALTER TABLE providers ADD COLUMN limit_daily_usd TEXT",
        "ALTER TABLE providers ADD COLUMN limit_monthly_usd TEXT",
        "ALTER TABLE providers ADD COLUMN provider_type TEXT",
        "ALTER TABLE skills ADD COLUMN enabled_hermes INTEGER NOT NULL DEFAULT 0",
    ];
    for sql in migrations {
        let _ = conn.execute(sql, []);
    }

    Ok(())
}
