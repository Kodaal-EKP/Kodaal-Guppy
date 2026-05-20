use crate::paths::{set_private_file, AppPaths};
use rand::{rngs::OsRng, RngCore};
use std::{fs, io};

pub fn load_or_create_token(paths: &AppPaths) -> io::Result<String> {
    if paths.token_path.exists() {
        let raw = fs::read_to_string(&paths.token_path)?;
        validate_token_file(&raw)?;
        Ok(raw.trim_end_matches('\n').to_string())
    } else {
        let token = generate_token();
        fs::write(&paths.token_path, format!("{token}\n"))?;
        set_private_file(&paths.token_path)?;
        Ok(token)
    }
}

pub fn is_valid_token_value(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_token_file(raw: &str) -> io::Result<()> {
    if !raw.ends_with('\n') || raw.matches('\n').count() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "token file must contain exactly 64 lowercase hex chars plus one trailing newline",
        ));
    }
    let value = raw.trim_end_matches('\n');
    if !is_valid_token_value(value) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "token must be exactly 64 lowercase hex chars",
        ));
    }
    Ok(())
}

pub fn generate_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::{generate_token, is_valid_token_value};

    #[test]
    fn token_validation_rejects_uppercase_and_short_values() {
        assert!(is_valid_token_value(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(!is_valid_token_value("abc"));
        assert!(!is_valid_token_value(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
    }

    #[test]
    fn test_nfr007_generated_token_is_256_bit_lowercase_hex() {
        let first = generate_token();
        let second = generate_token();
        assert!(is_valid_token_value(&first));
        assert!(is_valid_token_value(&second));
        assert_ne!(first, second);
    }

    #[cfg(windows)]
    #[test]
    fn test_nfr007_token_file_removes_windows_acl_inheritance() {
        use super::load_or_create_token;
        use crate::paths::AppPaths;
        use std::{fs, process::Command};

        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(dir.path().join("backups")).expect("backup dir");
        let paths = AppPaths {
            home_dir: dir.path().to_path_buf(),
            config_path: dir.path().join("config.toml"),
            token_path: dir.path().join("token"),
            db_path: dir.path().join("guppy.db"),
            backup_dir: dir.path().join("backups"),
            audit_log_path: dir.path().join("audit.log"),
        };
        load_or_create_token(&paths).expect("token");

        let output = Command::new("icacls")
            .arg(&paths.token_path)
            .output()
            .expect("icacls");
        assert!(output.status.success());
        let acl = String::from_utf8_lossy(&output.stdout);
        assert!(!acl.contains("(I)"), "{acl}");
    }
}
