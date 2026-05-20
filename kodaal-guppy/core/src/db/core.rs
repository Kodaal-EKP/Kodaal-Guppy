impl Database {
    pub fn open(paths: &AppPaths, config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
        let mut conn = Connection::open(&paths.db_path)?;
        apply_database_encryption(&mut conn, config)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        apply_migrations(&mut conn, paths, config)?;
        Ok(Self {
            conn,
            dedup_window_seconds: config.capture.dedup_window_seconds,
            max_prompt_length: config.capture.max_prompt_length,
        })
    }

    pub fn schema_version(&self) -> Result<i64, ApiError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0))
    }

    pub fn watcher_offset(&self, source_app: &str, path: &str) -> Result<u64, ApiError> {
        let value = self
            .conn
            .query_row(
                "SELECT offset_bytes FROM watcher_offsets WHERE source_app = ?1 AND path = ?2",
                params![source_app, path],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(value.max(0) as u64)
    }

    pub fn set_watcher_offset(
        &mut self,
        source_app: &str,
        path: &str,
        offset: u64,
    ) -> Result<(), ApiError> {
        self.conn.execute(
            "INSERT INTO watcher_offsets (source_app, path, offset_bytes, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_app, path)
             DO UPDATE SET offset_bytes = excluded.offset_bytes, updated_at = excluded.updated_at",
            params![source_app, path, offset as i64, ids::now_iso()],
        )?;
        Ok(())
    }

    pub fn set_dedup_window_seconds(&mut self, seconds: u32) {
        self.dedup_window_seconds = seconds;
    }
}
