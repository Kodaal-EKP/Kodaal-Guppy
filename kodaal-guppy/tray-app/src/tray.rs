#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMode {
    Starting,
    Capturing,
    Paused,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptPreview {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    pub version: String,
    pub prompts_today: u64,
    pub mode: TrayMode,
    pub recent_prompts: Vec<PromptPreview>,
}

impl TraySnapshot {
    pub fn starting(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            prompts_today: 0,
            mode: TrayMode::Starting,
            recent_prompts: Vec::new(),
        }
    }

    pub fn error(version: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            prompts_today: 0,
            mode: TrayMode::Error(reason.into()),
            recent_prompts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    Pause,
    Resume,
    OpenUi,
    OpenSettings,
    ShowToken,
    About,
    Quit,
    ReusePrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Header {
        label: String,
    },
    Separator,
    Action {
        id: String,
        label: String,
        enabled: bool,
    },
    Submenu {
        id: String,
        label: String,
        items: Vec<MenuEntry>,
    },
}

pub fn tooltip(mode: &TrayMode) -> String {
    match mode {
        TrayMode::Capturing => "Kodaal Guppy - capturing".to_string(),
        TrayMode::Paused => "Kodaal Guppy - paused".to_string(),
        TrayMode::Starting => "Kodaal Guppy - starting".to_string(),
        TrayMode::Error(reason) => format!("Kodaal Guppy - error: {reason}"),
    }
}

pub fn toggle_label(mode: &TrayMode) -> Option<&'static str> {
    match mode {
        TrayMode::Capturing => Some("Pause capture"),
        TrayMode::Paused => Some("Resume capture"),
        TrayMode::Starting | TrayMode::Error(_) => None,
    }
}

pub fn toggle_action(mode: &TrayMode) -> Option<TrayAction> {
    match mode {
        TrayMode::Capturing => Some(TrayAction::Pause),
        TrayMode::Paused => Some(TrayAction::Resume),
        TrayMode::Starting | TrayMode::Error(_) => None,
    }
}

pub fn menu_model(snapshot: &TraySnapshot) -> Vec<MenuEntry> {
    let mut entries = vec![
        MenuEntry::Header {
            label: format!("Kodaal Guppy {}", snapshot.version),
        },
        MenuEntry::Header {
            label: prompts_today_label(snapshot.prompts_today),
        },
        MenuEntry::Separator,
    ];

    if let Some(label) = toggle_label(&snapshot.mode) {
        entries.push(MenuEntry::Action {
            id: action_id(&toggle_action(&snapshot.mode).expect("toggle action exists")),
            label: label.to_string(),
            enabled: true,
        });
    } else {
        entries.push(MenuEntry::Action {
            id: "refresh".to_string(),
            label: "Core unavailable".to_string(),
            enabled: false,
        });
    }

    entries.extend([
        MenuEntry::Separator,
        MenuEntry::Action {
            id: action_id(&TrayAction::OpenUi),
            label: "Open Guppy".to_string(),
            enabled: true,
        },
        recent_submenu(&snapshot.recent_prompts),
        MenuEntry::Separator,
        MenuEntry::Action {
            id: action_id(&TrayAction::ShowToken),
            label: "Show token".to_string(),
            enabled: true,
        },
        MenuEntry::Action {
            id: action_id(&TrayAction::OpenSettings),
            label: "Settings".to_string(),
            enabled: true,
        },
        MenuEntry::Separator,
        MenuEntry::Action {
            id: action_id(&TrayAction::About),
            label: "About Kodaal Guppy".to_string(),
            enabled: true,
        },
        MenuEntry::Action {
            id: action_id(&TrayAction::Quit),
            label: "Quit Kodaal Guppy".to_string(),
            enabled: true,
        },
    ]);

    entries
}

pub fn action_id(action: &TrayAction) -> String {
    match action {
        TrayAction::Pause => "pause".to_string(),
        TrayAction::Resume => "resume".to_string(),
        TrayAction::OpenUi => "open-ui".to_string(),
        TrayAction::OpenSettings => "open-settings".to_string(),
        TrayAction::ShowToken => "show-token".to_string(),
        TrayAction::About => "about".to_string(),
        TrayAction::Quit => "quit".to_string(),
        TrayAction::ReusePrompt(id) => format!("recent:{id}"),
    }
}

pub fn action_from_id(id: &str) -> Option<TrayAction> {
    match id {
        "pause" => Some(TrayAction::Pause),
        "resume" => Some(TrayAction::Resume),
        "open-ui" => Some(TrayAction::OpenUi),
        "open-settings" => Some(TrayAction::OpenSettings),
        "show-token" => Some(TrayAction::ShowToken),
        "about" => Some(TrayAction::About),
        "quit" => Some(TrayAction::Quit),
        _ => id
            .strip_prefix("recent:")
            .map(|prompt_id| TrayAction::ReusePrompt(prompt_id.to_string())),
    }
}

pub fn prompts_today_label(count: u64) -> String {
    match count {
        1 => "1 prompt captured today".to_string(),
        value => format!("{value} prompts captured today"),
    }
}

pub fn truncate_prompt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max_chars).collect();
    truncated.push_str("...");
    truncated
}

fn recent_submenu(prompts: &[PromptPreview]) -> MenuEntry {
    let mut items: Vec<MenuEntry> = prompts
        .iter()
        .take(5)
        .map(|prompt| MenuEntry::Action {
            id: action_id(&TrayAction::ReusePrompt(prompt.id.clone())),
            label: truncate_prompt(&prompt.text, 50),
            enabled: true,
        })
        .collect();

    if items.is_empty() {
        items.push(MenuEntry::Action {
            id: "recent-empty".to_string(),
            label: "No recent prompts".to_string(),
            enabled: false,
        });
    }

    items.push(MenuEntry::Separator);
    items.push(MenuEntry::Action {
        id: action_id(&TrayAction::OpenUi),
        label: "Show all in Guppy".to_string(),
        enabled: true,
    });

    MenuEntry::Submenu {
        id: "recent".to_string(),
        label: "Recent prompts".to_string(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fr102_capturing_toggle_maps_to_pause() {
        assert_eq!(toggle_label(&TrayMode::Capturing), Some("Pause capture"));
        assert_eq!(toggle_action(&TrayMode::Capturing), Some(TrayAction::Pause));
        assert_eq!(tooltip(&TrayMode::Capturing), "Kodaal Guppy - capturing");
    }

    #[test]
    fn test_fr102_paused_toggle_maps_to_resume() {
        assert_eq!(toggle_label(&TrayMode::Paused), Some("Resume capture"));
        assert_eq!(toggle_action(&TrayMode::Paused), Some(TrayAction::Resume));
        assert_eq!(tooltip(&TrayMode::Paused), "Kodaal Guppy - paused");
    }

    #[test]
    fn test_fr102_menu_model_limits_recent_prompts_and_keeps_action_ids() {
        let snapshot = TraySnapshot {
            version: "0.1.0".to_string(),
            prompts_today: 24,
            mode: TrayMode::Capturing,
            recent_prompts: (0..8)
                .map(|index| PromptPreview {
                    id: format!("prompt-{index}"),
                    text: format!("prompt text {index} {}", "x".repeat(80)),
                })
                .collect(),
        };

        let menu = menu_model(&snapshot);
        let recent = menu.iter().find_map(|entry| match entry {
            MenuEntry::Submenu { id, items, .. } if id == "recent" => Some(items),
            _ => None,
        });
        let recent = recent.expect("recent submenu");
        let prompt_items: Vec<_> = recent
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Action { id, label, .. } if id.starts_with("recent:") => {
                    Some((id.as_str(), label.as_str()))
                }
                _ => None,
            })
            .collect();

        assert_eq!(prompt_items.len(), 5);
        assert_eq!(prompt_items[0].0, "recent:prompt-0");
        assert!(prompt_items[0].1.ends_with("..."));
        assert!(prompt_items[0].1.chars().count() <= 53);
    }

    #[test]
    fn test_fr102_action_ids_round_trip() {
        let actions = [
            TrayAction::Pause,
            TrayAction::Resume,
            TrayAction::OpenUi,
            TrayAction::OpenSettings,
            TrayAction::ShowToken,
            TrayAction::About,
            TrayAction::Quit,
            TrayAction::ReusePrompt("abc".to_string()),
        ];
        for action in actions {
            assert_eq!(action_from_id(&action_id(&action)), Some(action));
        }
    }
}
