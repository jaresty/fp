#[cfg(test)]
mod tests {
    #[test]
    fn shell_governs_fps_function_content() {
        let content = crate::shell::fps_function_content("fish").expect("fish must be supported");
        assert!(
            content.contains("fp root"),
            "shell::fps_function_content fish must dispatch root: {}",
            content
        );
    }

    #[test]
    fn shell_governs_fps_install_path() {
        let path = crate::shell::fps_install_path("fish").expect("fish install path must exist");
        assert!(
            path.to_string_lossy().contains(".config/fish/functions/fps.fish"),
            "shell::fps_install_path fish must be in .config/fish/functions: {}",
            path.display()
        );
    }

    #[test]
    fn shell_governs_detect_shell() {
        let shell = crate::shell::detect_shell();
        assert!(
            !shell.is_empty(),
            "shell::detect_shell must return a non-empty string"
        );
    }
}
