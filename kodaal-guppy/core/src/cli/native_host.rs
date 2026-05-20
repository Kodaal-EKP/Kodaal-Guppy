fn native_token_host() -> Result<i32, CliFailure> {
    let mut length = [0_u8; 4];
    std::io::stdin()
        .read_exact(&mut length)
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    let size = u32::from_le_bytes(length) as usize;
    if size > 4096 {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: "native message too large".to_string(),
        });
    }
    let mut message = vec![0_u8; size];
    std::io::stdin()
        .read_exact(&mut message)
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    let token = fs::read_to_string(&paths.token_path)
        .map_err(|error| CliFailure::generic(error.to_string()))?
        .trim()
        .to_string();
    if !crate::auth::is_valid_token_value(&token) {
        return Err(CliFailure {
            code: EXIT_AUTH,
            message: "token file contains an invalid token".to_string(),
        });
    }
    let response = json!({ "token": token }).to_string().into_bytes();
    std::io::stdout()
        .write_all(&(response.len() as u32).to_le_bytes())
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    std::io::stdout()
        .write_all(&response)
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    Ok(0)
}

fn native_token_host_exit_code() -> i32 {
    match native_token_host() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", error.message);
            error.code
        }
    }
}

fn is_native_token_host_invocation() -> bool {
    env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|value| value.to_owned()))
        .and_then(|value| value.to_str().map(|value| value.to_string()))
        .map(|stem| stem.eq_ignore_ascii_case("kodaal-native-token-host"))
        .unwrap_or(false)
}

const NATIVE_HOST_NAME: &str = "com.kodaal.guppy";
const CHROMIUM_EXTENSION_ID: &str = "mkpophigojljcfcfmpeljfhpokodfnlm";
const FIREFOX_EXTENSION_ID: &str = "guppy@kodaal.local";

fn native_host_binary_filename() -> &'static str {
    if cfg!(windows) {
        "kodaal-native-token-host.exe"
    } else {
        "kodaal-native-token-host"
    }
}

fn install_native_host_manifests(
    paths: &AppPaths,
    host_path: &Path,
) -> Result<Vec<PathBuf>, CliFailure> {
    let manifest_dir = paths.home_dir.join("native-messaging");
    fs::create_dir_all(&manifest_dir).map_err(|error| CliFailure::generic(error.to_string()))?;
    let chromium_manifest = manifest_dir.join("chromium-com.kodaal.guppy.json");
    let firefox_manifest = manifest_dir.join("firefox-com.kodaal.guppy.json");
    write_native_host_manifest(&chromium_manifest, host_path, NativeBrowserFamily::Chromium)?;
    write_native_host_manifest(&firefox_manifest, host_path, NativeBrowserFamily::Firefox)?;

    if cfg!(windows) {
        register_windows_native_host("Google\\Chrome", &chromium_manifest)?;
        register_windows_native_host("Chromium", &chromium_manifest)?;
        register_windows_native_host("Microsoft\\Edge", &chromium_manifest)?;
        register_windows_native_host("BraveSoftware\\Brave-Browser", &chromium_manifest)?;
        register_windows_native_host("Mozilla", &firefox_manifest)?;
        Ok(vec![chromium_manifest, firefox_manifest])
    } else {
        let mut written = Vec::new();
        for target in native_host_install_paths(NativeBrowserFamily::Chromium) {
            copy_manifest(&chromium_manifest, &target)?;
            written.push(target);
        }
        for target in native_host_install_paths(NativeBrowserFamily::Firefox) {
            copy_manifest(&firefox_manifest, &target)?;
            written.push(target);
        }
        Ok(written)
    }
}

fn uninstall_native_host_manifests(paths: &AppPaths) {
    if cfg!(windows) {
        unregister_windows_native_host("Google\\Chrome");
        unregister_windows_native_host("Chromium");
        unregister_windows_native_host("Microsoft\\Edge");
        unregister_windows_native_host("BraveSoftware\\Brave-Browser");
        unregister_windows_native_host("Mozilla");
    }
    let _ = fs::remove_dir_all(paths.home_dir.join("native-messaging"));
    for family in [NativeBrowserFamily::Chromium, NativeBrowserFamily::Firefox] {
        for target in native_host_install_paths(family) {
            let _ = fs::remove_file(target);
        }
    }
}

#[derive(Clone, Copy)]
enum NativeBrowserFamily {
    Chromium,
    Firefox,
}

fn write_native_host_manifest(
    path: &Path,
    host_path: &Path,
    family: NativeBrowserFamily,
) -> Result<(), CliFailure> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| CliFailure::generic(error.to_string()))?;
    }
    fs::write(path, native_host_manifest(host_path, family))
        .map_err(|error| CliFailure::generic(error.to_string()))
}

fn native_host_manifest(host_path: &Path, family: NativeBrowserFamily) -> String {
    let host = host_path.to_string_lossy();
    let manifest = match family {
        NativeBrowserFamily::Chromium => json!({
            "name": NATIVE_HOST_NAME,
            "description": "Kodaal Guppy local token host",
            "path": host,
            "type": "stdio",
            "allowed_origins": [format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")]
        }),
        NativeBrowserFamily::Firefox => json!({
            "name": NATIVE_HOST_NAME,
            "description": "Kodaal Guppy local token host",
            "path": host,
            "type": "stdio",
            "allowed_extensions": [FIREFOX_EXTENSION_ID]
        }),
    };
    serde_json::to_string_pretty(&manifest).expect("native host manifest serializes")
}

fn copy_manifest(source: &Path, target: &Path) -> Result<(), CliFailure> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| CliFailure::generic(error.to_string()))?;
    }
    fs::copy(source, target)
        .map(|_| ())
        .map_err(|error| CliFailure::generic(error.to_string()))
}

fn native_host_install_paths(family: NativeBrowserFamily) -> Vec<PathBuf> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from));
    let Some(home) = home else {
        return Vec::new();
    };
    let file = format!("{NATIVE_HOST_NAME}.json");
    if cfg!(target_os = "macos") {
        return match family {
            NativeBrowserFamily::Chromium => vec![
                home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
                    .join(&file),
                home.join("Library/Application Support/Chromium/NativeMessagingHosts")
                    .join(&file),
                home.join("Library/Application Support/Microsoft Edge/NativeMessagingHosts")
                    .join(&file),
                home.join("Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts")
                    .join(&file),
            ],
            NativeBrowserFamily::Firefox => vec![
                home.join("Library/Application Support/Mozilla/NativeMessagingHosts")
                    .join(&file),
            ],
        };
    }
    if cfg!(target_os = "linux") {
        return match family {
            NativeBrowserFamily::Chromium => vec![
                home.join(".config/google-chrome/NativeMessagingHosts")
                    .join(&file),
                home.join(".config/chromium/NativeMessagingHosts").join(&file),
                home.join(".config/microsoft-edge/NativeMessagingHosts")
                    .join(&file),
                home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
                    .join(&file),
            ],
            NativeBrowserFamily::Firefox => {
                vec![home.join(".mozilla/native-messaging-hosts").join(&file)]
            }
        };
    }
    Vec::new()
}

fn register_windows_native_host(browser_key: &str, manifest_path: &Path) -> Result<(), CliFailure> {
    let registry_key = format!(
        "HKCU\\Software\\{}\\NativeMessagingHosts\\{}",
        browser_key, NATIVE_HOST_NAME
    );
    let manifest = manifest_path.to_string_lossy().to_string();
    let status = Command::new("reg")
        .args([
            "add",
            &registry_key,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &manifest,
            "/f",
        ])
        .status()
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    if !status.success() {
        return Err(CliFailure::generic(format!(
            "failed to register native messaging host for {browser_key}"
        )));
    }
    Ok(())
}

fn unregister_windows_native_host(browser_key: &str) {
    let registry_key = format!(
        "HKCU\\Software\\{}\\NativeMessagingHosts\\{}",
        browser_key, NATIVE_HOST_NAME
    );
    let _ = Command::new("reg")
        .args(["delete", &registry_key, "/f"])
        .status();
}
