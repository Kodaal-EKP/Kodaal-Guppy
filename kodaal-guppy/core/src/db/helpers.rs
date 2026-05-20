fn apply_migrations(
    conn: &mut Connection,
    paths: &AppPaths,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS refinery_schema_history (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_on TEXT NOT NULL
        )",
        [],
    )?;
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
        [],
        |row| row.get(0),
    )?;
    if version < 1 {
        if config.backups.pre_migration_backup
            && paths.db_path.exists()
            && fs::metadata(&paths.db_path)?.len() > 0
        {
            backup_before_migrate(paths, 1, config.backups.keep_count as usize)?;
        }
        conn.execute_batch(V001_SQL)?;
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on) VALUES (1, 'V001__initial_schema', ?1)",
            params![ids::now_iso()],
        )?;
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
        [],
        |row| row.get(0),
    )?;
    if version < 2 {
        if config.backups.pre_migration_backup
            && paths.db_path.exists()
            && fs::metadata(&paths.db_path)?.len() > 0
        {
            backup_before_migrate(paths, 2, config.backups.keep_count as usize)?;
        }
        conn.execute_batch(V002_SQL)?;
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on) VALUES (2, 'V002__add_artifacts', ?1)",
            params![ids::now_iso()],
        )?;
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
        [],
        |row| row.get(0),
    )?;
    if version < 3 {
        if config.backups.pre_migration_backup
            && paths.db_path.exists()
            && fs::metadata(&paths.db_path)?.len() > 0
        {
            backup_before_migrate(paths, 3, config.backups.keep_count as usize)?;
        }
        conn.execute_batch(V003_SQL)?;
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on) VALUES (3, 'V003__watcher_offsets', ?1)",
            params![ids::now_iso()],
        )?;
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
        [],
        |row| row.get(0),
    )?;
    if version < 4 {
        if config.backups.pre_migration_backup
            && paths.db_path.exists()
            && fs::metadata(&paths.db_path)?.len() > 0
        {
            backup_before_migrate(paths, 4, config.backups.keep_count as usize)?;
        }
        conn.execute_batch(V004_SQL)?;
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on) VALUES (4, 'V004__add_desktop_source', ?1)",
            params![ids::now_iso()],
        )?;
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
        [],
        |row| row.get(0),
    )?;
    if version < 5 {
        if config.backups.pre_migration_backup
            && paths.db_path.exists()
            && fs::metadata(&paths.db_path)?.len() > 0
        {
            backup_before_migrate(paths, 5, config.backups.keep_count as usize)?;
        }
        conn.execute_batch(V005_SQL)?;
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on) VALUES (5, 'V005__prompt_redaction', ?1)",
            params![ids::now_iso()],
        )?;
    }
    Ok(())
}

fn apply_database_encryption(
    conn: &mut Connection,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    if config.database.encryption == "off" {
        return Ok(());
    }
    let key = std::env::var(&config.database.key_env).map_err(|_| {
        format!(
            "database encryption is sqlcipher but {} is not set",
            config.database.key_env
        )
    })?;
    if key.is_empty() {
        return Err(format!("{} must not be empty", config.database.key_env).into());
    }
    conn.pragma_update(None, "key", key.as_str())?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(())
}

fn backup_before_migrate(
    paths: &AppPaths,
    target_version: i64,
    keep_count: usize,
) -> std::io::Result<()> {
    fs::create_dir_all(&paths.backup_dir)?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S");
    let backup = paths
        .backup_dir
        .join(format!("guppy-pre-V{target_version:03}-{timestamp}.db"));
    fs::copy(&paths.db_path, backup)?;
    let mut backups = fs::read_dir(&paths.backup_dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("guppy-pre-V")
        })
        .collect::<Vec<_>>();
    backups.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());
    while backups.len() > keep_count {
        if let Some(entry) = backups.first() {
            let _ = fs::remove_file(entry.path());
        }
        backups.remove(0);
    }
    Ok(())
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        color: row.get(3)?,
        prompt_count: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    Ok(Artifact {
        id: row.get(0)?,
        prompt_id: row.get(1)?,
        filename: row.get(2)?,
        original_path: row.get(3)?,
        project_id: row.get(4)?,
        storage_mode: row.get(5)?,
        snapshot_size: row.get(6)?,
        mime_type: row.get(7)?,
        detection_mode: row.get(8)?,
        is_broken: row.get::<_, i64>(9)? == 1,
        created_at: row.get(10)?,
        last_verified_at: row.get(11)?,
    })
}

fn validate_storage_mode(value: &str) -> Result<(), ApiError> {
    match value {
        "reference" | "snapshot" => Ok(()),
        _ => Err(ApiError::invalid_payload(
            "storage_mode must be reference or snapshot",
            Some("storage_mode"),
        )),
    }
}

fn read_limited_file(path: &Path) -> Result<Vec<u8>, ApiError> {
    let metadata =
        fs::metadata(path).map_err(|_| ApiError::not_found("FILE_NOT_FOUND", "file not found"))?;
    if metadata.len() > 5 * 1024 * 1024 {
        return Err(ApiError::too_large(
            "ARTIFACT_TOO_LARGE",
            "snapshot file exceeds 5 MB",
        ));
    }
    fs::read(path).map_err(|error| ApiError::internal(error.to_string()))
}

fn next_available_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 2..10_000 {
        let filename = if let Some(extension) = extension {
            format!("{stem}-{index}.{extension}")
        } else {
            format!("{stem}-{index}")
        };
        let candidate = parent.join(filename);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}-copy"))
}

fn sql_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::from(value),
        ValueRef::Real(value) => Value::from(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => Value::String(hex::encode(value)),
    }
}

fn default_storage_mode() -> String {
    "reference".to_string()
}

fn default_on_conflict() -> String {
    "prompt".to_string()
}

fn validate_source(value: &str) -> Result<(), ApiError> {
    match value {
        "browser" | "desktop" | "ide" | "cli" | "mcp" => Ok(()),
        _ => Err(ApiError::invalid_payload("invalid source", Some("source"))),
    }
}

fn validate_project_hint(hint: &ProjectHint) -> Result<(), ApiError> {
    match hint.kind.as_str() {
        "path" | "cwd" | "domain" => {}
        _ => {
            return Err(ApiError::invalid_payload(
                "invalid project_hint.type",
                Some("project_hint.type"),
            ))
        }
    }
    if hint.value.trim().is_empty() {
        return Err(ApiError::invalid_payload(
            "project_hint.value must not be empty",
            Some("project_hint.value"),
        ));
    }
    Ok(())
}

fn validate_iso_like(value: &str, field: &'static str) -> Result<(), ApiError> {
    if value.len() < 10 || !value.contains('T') {
        return Err(ApiError::invalid_payload(
            "timestamp must be ISO 8601 UTC",
            Some(field),
        ));
    }
    Ok(())
}

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
}

fn fts_query(input: &str) -> Option<String> {
    let words = input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| format!("{}*", part.to_lowercase()))
        .collect::<Vec<_>>();
    if words.is_empty() {
        None
    } else {
        Some(words.join(" AND "))
    }
}
