use crate::{
    api_client::{TrayApi, TrayClientError},
    tray::{TrayAction, TrayMode, TraySnapshot},
};

pub const UI_URL: &str = "http://127.0.0.1:7878/ui";
pub const SETTINGS_URL: &str = "http://127.0.0.1:7878/ui/settings";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    Snapshot(TraySnapshot),
    OpenUrl(&'static str),
    ShowToken(String),
    About,
    Quit,
    Error(String),
}

pub struct TrayController<C> {
    api: C,
    snapshot: TraySnapshot,
}

impl<C: TrayApi> TrayController<C> {
    pub fn new(api: C, version: impl Into<String>) -> Self {
        Self {
            api,
            snapshot: TraySnapshot::starting(version),
        }
    }

    pub fn snapshot(&self) -> &TraySnapshot {
        &self.snapshot
    }

    pub fn refresh(&mut self) -> EventOutcome {
        self.apply_result(self.api.snapshot())
    }

    pub fn handle_action(&mut self, action: TrayAction) -> EventOutcome {
        match action {
            TrayAction::Pause => self.apply_result(self.api.pause_capture()),
            TrayAction::Resume => self.apply_result(self.api.resume_capture()),
            TrayAction::OpenUi => EventOutcome::OpenUrl(UI_URL),
            TrayAction::OpenSettings => EventOutcome::OpenUrl(SETTINGS_URL),
            TrayAction::ShowToken => match self.api.token() {
                Ok(token) => EventOutcome::ShowToken(token),
                Err(error) => self.apply_error(error),
            },
            TrayAction::About => EventOutcome::About,
            TrayAction::Quit => EventOutcome::Quit,
            TrayAction::ReusePrompt(id) => match self.api.reuse_prompt(&id) {
                Ok(()) => self.refresh(),
                Err(error) => self.apply_error(error),
            },
        }
    }

    fn apply_result(&mut self, result: Result<TraySnapshot, TrayClientError>) -> EventOutcome {
        match result {
            Ok(snapshot) => {
                self.snapshot = snapshot.clone();
                EventOutcome::Snapshot(snapshot)
            }
            Err(error) => self.apply_error(error),
        }
    }

    fn apply_error(&mut self, error: TrayClientError) -> EventOutcome {
        self.snapshot.mode = TrayMode::Error(error.message.clone());
        EventOutcome::Error(error.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::{PromptPreview, TrayMode};
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeApi {
        paused: RefCell<bool>,
        reused: RefCell<Vec<String>>,
        fail_snapshot: bool,
    }

    impl TrayApi for FakeApi {
        fn snapshot(&self) -> Result<TraySnapshot, TrayClientError> {
            if self.fail_snapshot {
                return Err(TrayClientError {
                    code: "CORE_DOWN".to_string(),
                    message: "core unavailable".to_string(),
                });
            }
            Ok(TraySnapshot {
                version: "0.1.0".to_string(),
                prompts_today: 3,
                mode: if *self.paused.borrow() {
                    TrayMode::Paused
                } else {
                    TrayMode::Capturing
                },
                recent_prompts: vec![PromptPreview {
                    id: "p1".to_string(),
                    text: "prompt one".to_string(),
                }],
            })
        }

        fn pause_capture(&self) -> Result<TraySnapshot, TrayClientError> {
            *self.paused.borrow_mut() = true;
            self.snapshot()
        }

        fn resume_capture(&self) -> Result<TraySnapshot, TrayClientError> {
            *self.paused.borrow_mut() = false;
            self.snapshot()
        }

        fn reuse_prompt(&self, id: &str) -> Result<(), TrayClientError> {
            self.reused.borrow_mut().push(id.to_string());
            Ok(())
        }

        fn token(&self) -> Result<String, TrayClientError> {
            Ok("a".repeat(64))
        }
    }

    #[test]
    fn test_fr102_controller_pause_and_resume_call_api() {
        let api = FakeApi::default();
        let mut controller = TrayController::new(api, "0.1.0");

        assert!(matches!(
            controller.handle_action(TrayAction::Pause),
            EventOutcome::Snapshot(TraySnapshot {
                mode: TrayMode::Paused,
                ..
            })
        ));
        assert!(matches!(
            controller.handle_action(TrayAction::Resume),
            EventOutcome::Snapshot(TraySnapshot {
                mode: TrayMode::Capturing,
                ..
            })
        ));
    }

    #[test]
    fn test_fr102_controller_reuse_refreshes_snapshot() {
        let api = FakeApi::default();
        let mut controller = TrayController::new(api, "0.1.0");

        let outcome = controller.handle_action(TrayAction::ReusePrompt("p1".to_string()));

        assert!(matches!(outcome, EventOutcome::Snapshot(_)));
        assert_eq!(controller.snapshot().recent_prompts[0].id, "p1");
    }

    #[test]
    fn test_fr102_controller_errors_move_tray_to_error_mode() {
        let api = FakeApi {
            fail_snapshot: true,
            ..FakeApi::default()
        };
        let mut controller = TrayController::new(api, "0.1.0");

        assert_eq!(
            controller.refresh(),
            EventOutcome::Error("core unavailable".to_string())
        );
        assert_eq!(
            controller.snapshot().mode,
            TrayMode::Error("core unavailable".to_string())
        );
    }
}
