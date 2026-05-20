use super::*;

#[test]
fn test_config_rejects_unknown_top_level_sections() {
    let result = toml::from_str::<Config>(
        r#"
        [server]
        port = 7878
        bind_address = "127.0.0.1"

        [ui]
        theme = "dark"
        "#,
    );

    assert!(result.is_err());
}

#[test]
fn test_nfr006_config_rejects_non_loopback_bind_address() {
    let config = Config {
        server: ServerConfig {
            bind_address: "0.0.0.0".to_string(),
            ..ServerConfig::default()
        },
        ..Config::default()
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_fr110_config_rejects_invalid_database_encryption_mode() {
    let config = Config {
        database: DatabaseConfig {
            encryption: "plaintext".to_string(),
            key_env: "KODAAL_DB_KEY".to_string(),
        },
        ..Config::default()
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_fr065_config_rejects_invalid_prune_defaults() {
    let config = Config {
        prune: PruneConfig {
            older_than_days: Some(0),
            ..PruneConfig::default()
        },
        ..Config::default()
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_fr111_config_rejects_invalid_suggestion_defaults() {
    let config = Config {
        suggestions: SuggestionsConfig {
            min_chars: 2,
            ..SuggestionsConfig::default()
        },
        ..Config::default()
    };

    assert!(config.validate().is_err());
}
