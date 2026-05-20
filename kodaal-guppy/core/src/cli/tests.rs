#[cfg(test)]
mod tests {
    use super::{
        native_host_manifest, replace_shell_hook_block, shell_hook_block, ArtifactCommand, Cli,
        Commands, CompletionShell, ConfigCommand, NativeBrowserFamily, ProjectCommand, ShellHook,
    };
    use clap::Parser;
    use std::path::Path;

    #[test]
    fn parses_new_cli_parity_commands() {
        assert!(matches!(
            Cli::try_parse_from(["kodaal", "tag", "p1", "refactor"])
                .expect("tag")
                .command,
            Some(Commands::Tag(_))
        ));
        assert!(matches!(
            Cli::try_parse_from(["kodaal", "untag", "p1", "refactor"])
                .expect("untag")
                .command,
            Some(Commands::Untag(_))
        ));
        assert!(matches!(
            Cli::try_parse_from(["kodaal", "prune", "--shorter-than", "10", "--dry-run"])
                .expect("prune")
                .command,
            Some(Commands::Prune(_))
        ));
        assert!(matches!(
            Cli::try_parse_from(["kodaal", "import", "archive.json"])
                .expect("import")
                .command,
            Some(Commands::Import(_))
        ));
        assert!(matches!(
            Cli::try_parse_from(["kodaal", "suggest", "--source", "cli", "refactor", "rust"])
                .expect("suggest")
                .command,
            Some(Commands::Suggest(_))
        ));
    }

    #[test]
    fn parses_nested_project_artifact_config_and_completion_commands() {
        let parsed = Cli::try_parse_from(["kodaal", "projects", "rename", "p1", "new-name"])
            .expect("projects rename");
        assert!(matches!(
            parsed.command,
            Some(Commands::Projects(args))
                if matches!(args.command, Some(ProjectCommand::Rename { .. }))
        ));

        let parsed =
            Cli::try_parse_from(["kodaal", "artifact", "content", "a1", "--output", "out.txt"])
                .expect("artifact content");
        assert!(matches!(
            parsed.command,
            Some(Commands::Artifact(args))
                if matches!(args.command, ArtifactCommand::Content { .. })
        ));

        let parsed = Cli::try_parse_from(["kodaal", "config", "validate"]).expect("config");
        assert!(matches!(
            parsed.command,
            Some(Commands::Config(args)) if matches!(args.command, ConfigCommand::Validate)
        ));

        let parsed =
            Cli::try_parse_from(["kodaal", "completion", "powershell"]).expect("completion");
        assert!(matches!(
            parsed.command,
            Some(Commands::Completion(args)) if matches!(args.shell, CompletionShell::Powershell)
        ));

        let parsed = Cli::try_parse_from(["kodaal", "install-shell-hook", "--shell", "bash"])
            .expect("install shell hook");
        assert!(matches!(
            parsed.command,
            Some(Commands::InstallShellHook(args)) if matches!(args.shell, Some(ShellHook::Bash))
        ));

        let parsed = Cli::try_parse_from(["kodaal", "capture-shell", "codex-cli", "hello"])
            .expect("capture shell");
        assert!(matches!(parsed.command, Some(Commands::CaptureShell(_))));
    }

    #[test]
    fn shell_hook_replaces_existing_managed_block() {
        let block = shell_hook_block(&["claude".to_string()]);
        let first = replace_shell_hook_block("export PATH=$PATH\n", &block);
        let second = replace_shell_hook_block(&first, &block);
        assert_eq!(second.matches(">>> kodaal guppy shell hook").count(), 1);
        assert!(second.contains("[ -t 2 ] || return 0"));
        assert!(second.contains("1>&2 2>/dev/null"));
        assert!(second.contains("__kodaal_suggest_shell claude-code \"$@\""));
        assert!(second.contains("command claude \"$@\""));
    }

    #[test]
    fn test_fr106_native_messaging_manifests_allow_fixed_extension_ids() {
        let chromium =
            native_host_manifest(Path::new("/opt/kodaal/kodaal-native-token-host"), NativeBrowserFamily::Chromium);
        assert!(chromium.contains("\"allowed_origins\""));
        assert!(chromium.contains("chrome-extension://mkpophigojljcfcfmpeljfhpokodfnlm/"));
        assert!(chromium.contains("kodaal-native-token-host"));

        let firefox =
            native_host_manifest(Path::new("/opt/kodaal/kodaal-native-token-host"), NativeBrowserFamily::Firefox);
        assert!(firefox.contains("\"allowed_extensions\""));
        assert!(firefox.contains("guppy@kodaal.local"));
    }
}
