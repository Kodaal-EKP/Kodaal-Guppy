#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
fn main() {
    if let Err(error) = desktop::run() {
        eprintln!("kodaal-tray failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(feature = "desktop")]
mod desktop {
    use kodaal_tray::{
        api_client::{GuppyApiClient, TrayApi, TrayClientError},
        events::{EventOutcome, TrayController},
        tray::{self, TrayAction, TraySnapshot},
    };
    use std::{
        process::Command,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tauri::{
        menu::{MenuBuilder, SubmenuBuilder},
        tray::TrayIconBuilder,
        AppHandle,
    };

    type DesktopController = TrayController<Box<dyn TrayApi + Send + Sync>>;

    pub fn run() -> tauri::Result<()> {
        tauri::Builder::default()
            .setup(|app| {
                let api: Box<dyn TrayApi + Send + Sync> = match GuppyApiClient::from_default_paths()
                {
                    Ok(api) => Box::new(api),
                    Err(error) => Box::new(UnavailableApi {
                        code: error.code,
                        message: error.message,
                    }),
                };
                let mut controller =
                    TrayController::new(api, app.package_info().version.to_string());
                controller.refresh();
                let controller = Arc::new(Mutex::new(controller));
                install_tray(app.handle(), controller.clone())?;
                spawn_refresh_loop(app.handle().clone(), controller);
                Ok(())
            })
            .run(tauri::generate_context!())
    }

    struct UnavailableApi {
        code: String,
        message: String,
    }

    impl UnavailableApi {
        fn error(&self) -> TrayClientError {
            TrayClientError {
                code: self.code.clone(),
                message: self.message.clone(),
            }
        }
    }

    impl TrayApi for UnavailableApi {
        fn snapshot(&self) -> Result<TraySnapshot, TrayClientError> {
            Err(self.error())
        }

        fn pause_capture(&self) -> Result<TraySnapshot, TrayClientError> {
            Err(self.error())
        }

        fn resume_capture(&self) -> Result<TraySnapshot, TrayClientError> {
            Err(self.error())
        }

        fn reuse_prompt(&self, _id: &str) -> Result<(), TrayClientError> {
            Err(self.error())
        }

        fn token(&self) -> Result<String, TrayClientError> {
            Err(self.error())
        }
    }

    fn install_tray(
        app: &AppHandle,
        controller: Arc<Mutex<DesktopController>>,
    ) -> tauri::Result<()> {
        let snapshot = controller
            .lock()
            .map(|guard| guard.snapshot().clone())
            .unwrap_or_else(|_| {
                TraySnapshot::error(env!("CARGO_PKG_VERSION"), "tray state lock poisoned")
            });
        let menu = build_menu(app, &snapshot)?;
        let tray_controller = controller.clone();
        TrayIconBuilder::with_id("kodaal-guppy")
            .tooltip(tray::tooltip(&snapshot.mode))
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(move |app, event| {
                let id = event.id().0.as_str();
                if let Some(action) = tray::action_from_id(id) {
                    handle_action(app, &tray_controller, action);
                }
            })
            .build(app)?;
        Ok(())
    }

    fn rebuild_tray(app: &AppHandle, snapshot: &TraySnapshot) {
        let Some(tray) = app.tray_by_id("kodaal-guppy") else {
            return;
        };
        if let Ok(menu) = build_menu(app, snapshot) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(tray::tooltip(&snapshot.mode)));
    }

    fn build_menu(
        app: &AppHandle,
        snapshot: &TraySnapshot,
    ) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
        let recent = snapshot
            .recent_prompts
            .iter()
            .take(5)
            .fold(
                SubmenuBuilder::new(app, "Recent prompts"),
                |builder, prompt| {
                    builder.text(
                        tray::action_id(&TrayAction::ReusePrompt(prompt.id.clone())),
                        tray::truncate_prompt(&prompt.text, 50),
                    )
                },
            )
            .text(tray::action_id(&TrayAction::OpenUi), "Show all in Guppy")
            .build()?;

        let mut builder = MenuBuilder::new(app)
            .text("header", format!("Kodaal Guppy {}", snapshot.version))
            .text(
                "subtitle",
                tray::prompts_today_label(snapshot.prompts_today),
            )
            .separator();

        builder = match tray::toggle_label(&snapshot.mode) {
            Some(label) => builder.text(
                tray::action_id(&tray::toggle_action(&snapshot.mode).expect("toggle action")),
                label,
            ),
            None => builder.text("core-unavailable", "Core unavailable"),
        };

        builder
            .separator()
            .text(tray::action_id(&TrayAction::OpenUi), "Open Guppy")
            .item(&recent)
            .separator()
            .text(tray::action_id(&TrayAction::ShowToken), "Show token")
            .text(tray::action_id(&TrayAction::OpenSettings), "Settings")
            .separator()
            .text(tray::action_id(&TrayAction::About), "About Kodaal Guppy")
            .text(tray::action_id(&TrayAction::Quit), "Quit Kodaal Guppy")
            .build()
    }

    fn handle_action(
        app: &AppHandle,
        controller: &Arc<Mutex<DesktopController>>,
        action: TrayAction,
    ) {
        if matches!(action, TrayAction::Quit) {
            app.exit(0);
            return;
        }

        let outcome = match controller.lock() {
            Ok(mut guard) => guard.handle_action(action),
            Err(_) => EventOutcome::Error("tray state lock poisoned".to_string()),
        };
        apply_outcome(app, controller, outcome);
    }

    fn apply_outcome(
        app: &AppHandle,
        controller: &Arc<Mutex<DesktopController>>,
        outcome: EventOutcome,
    ) {
        match outcome {
            EventOutcome::Snapshot(snapshot) => rebuild_tray(app, &snapshot),
            EventOutcome::OpenUrl(url) => open_url(url),
            EventOutcome::ShowToken(token) => show_message(
                "Kodaal token",
                &format!(
                    "This token authorizes local extensions to read prompts.\n\n{}",
                    token
                ),
            ),
            EventOutcome::About => show_message("About Kodaal Guppy", "Kodaal Guppy 0.1.0"),
            EventOutcome::Quit => app.exit(0),
            EventOutcome::Error(message) => {
                let snapshot = controller
                    .lock()
                    .map(|guard| guard.snapshot().clone())
                    .unwrap_or_else(|_| {
                        TraySnapshot::error(env!("CARGO_PKG_VERSION"), message.clone())
                    });
                rebuild_tray(app, &snapshot);
                show_message("Kodaal Guppy", &message);
            }
        }
    }

    fn spawn_refresh_loop(app: AppHandle, controller: Arc<Mutex<DesktopController>>) {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let snapshot = match controller.lock() {
                Ok(mut guard) => match guard.refresh() {
                    EventOutcome::Snapshot(snapshot) => snapshot,
                    EventOutcome::Error(message) => {
                        TraySnapshot::error(env!("CARGO_PKG_VERSION"), message)
                    }
                    _ => guard.snapshot().clone(),
                },
                Err(_) => {
                    TraySnapshot::error(env!("CARGO_PKG_VERSION"), "tray state lock poisoned")
                }
            };
            rebuild_tray(&app, &snapshot);
        });
    }

    fn open_url(url: &str) {
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "start", "", url]);
            command
        };
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = Command::new("open");
            command.arg(url);
            command
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut command = {
            let mut command = Command::new("xdg-open");
            command.arg(url);
            command
        };
        let _ = command.spawn();
    }

    fn show_message(title: &str, message: &str) {
        #[cfg(target_os = "windows")]
        {
            let script = format!(
                "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show(@'\n{}\n'@, '{}') | Out-Null",
                message.replace("'", "''"),
                title.replace("'", "''")
            );
            let _ = Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "display dialog \"{}\" with title \"{}\"",
                        message.replace('"', "\\\""),
                        title.replace('"', "\\\"")
                    ),
                ])
                .spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = Command::new("zenity")
                .args(["--info", "--title", title, "--text", message])
                .spawn();
        }
    }
}
