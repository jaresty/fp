#[cfg(test)]
mod tests {
    #[test]
    fn commands_governs_unlock_formats_success_message() {
        let result = crate::commands::unlock_message("feat-branch");
        assert!(
            result.contains("feat-branch"),
            "commands::unlock_message must include branch name: {}",
            result
        );
        assert!(
            result.contains("Unlocked"),
            "commands::unlock_message must say 'Unlocked': {}",
            result
        );
    }

    #[test]
    fn commands_governs_agent_context_text_output() {
        let result = crate::commands::agent_context_text(3);
        assert!(
            result.contains("3"),
            "commands::agent_context_text must include PR count: {}",
            result
        );
        assert!(
            result.contains("tracked"),
            "commands::agent_context_text must say 'tracked': {}",
            result
        );
    }

    #[test]
    fn commands_governs_install_skills_writes_skill_content() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("SKILL.md");
        crate::commands::install_skills(&dest).unwrap();
        assert!(dest.exists(), "commands::install_skills must create the file");
        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(
            content.contains("name: fp"),
            "commands::install_skills must write skill content with 'name: fp': {}",
            &content[..100.min(content.len())]
        );
    }

    #[test]
    fn commands_governs_install_shell_print_returns_content() {
        let result = crate::commands::install_shell_content("fish");
        assert!(
            result.is_ok(),
            "commands::install_shell_content must succeed for fish shell"
        );
        let content = result.unwrap();
        assert!(
            content.contains("fps"),
            "commands::install_shell_content must return fps function body: {}",
            &content[..50.min(content.len())]
        );
    }

    #[test]
    fn commands_governs_install_shell_unsupported_errors() {
        let result = crate::commands::install_shell_content("powershell");
        assert!(
            result.is_err(),
            "commands::install_shell_content must error for unsupported shell"
        );
        assert!(
            result.unwrap_err().to_string().contains("unsupported shell"),
            "error must mention 'unsupported shell'"
        );
    }
}
