use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub home_dir: PathBuf,
    pub config_path: PathBuf,
    pub token_path: PathBuf,
    pub db_path: PathBuf,
    pub backup_dir: PathBuf,
    pub audit_log_path: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> io::Result<Self> {
        let home_dir = if let Some(value) = env::var_os("KODAAL_HOME") {
            PathBuf::from(value)
        } else if let Some(value) = env::var_os("KODAAL_CONFIG") {
            let config_path = PathBuf::from(value);
            config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        } else {
            default_home_dir()?
        };
        let config_path = env::var_os("KODAAL_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir.join("config.toml"));
        Ok(Self {
            token_path: env::var_os("KODAAL_TOKEN_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir.join("token")),
            db_path: home_dir.join("guppy.db"),
            backup_dir: home_dir.join("backups"),
            audit_log_path: home_dir.join("audit.log"),
            config_path,
            home_dir,
        })
    }

    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.home_dir)?;
        fs::create_dir_all(&self.backup_dir)?;
        set_private_dir(&self.home_dir)?;
        set_private_dir(&self.backup_dir)?;
        Ok(())
    }
}

fn default_home_dir() -> io::Result<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            return Ok(PathBuf::from(appdata).join("Kodaal"));
        }
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".kodaal"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

#[cfg(unix)]
fn set_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(windows)]
fn set_private_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn set_private_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn set_private_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(windows)]
    {
        set_private_windows_acl(path)?;
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(windows)]
fn set_private_windows_acl(path: &Path) -> io::Result<()> {
    let output = Command::new("whoami").output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "whoami failed while resolving current user for ACL hardening",
        ));
    }
    let user = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if user.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "whoami returned an empty current user for ACL hardening",
        ));
    }
    run_icacls(path, &["/inheritance:r".to_string()])?;
    run_icacls(path, &["/grant:r".to_string(), format!("{user}:F")])
}

#[cfg(windows)]
fn run_icacls(path: &Path, args: &[String]) -> io::Result<()> {
    let status = Command::new("icacls")
        .arg(path)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("icacls failed for {}", path.display()),
        ))
    }
}
