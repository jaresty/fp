pub fn fps_function_content(shell: &str) -> Option<String> {
    match shell {
        "fish" => Some(r#"function fps
    if test "$argv[1]" = root
        cd (fp root)
    else
        set dir (fp switch $argv)
        and cd $dir
    end
end"#.to_string()),
        "zsh" | "bash" => Some(r#"fps() {
    if [ "$1" = root ]; then
        cd "$(fp root)"
    else
        local dir
        dir=$(fp switch "$@") && cd "$dir"
    fi
}"#.to_string()),
        _ => None,
    }
}

pub fn fps_install_path(shell: &str) -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    match shell {
        "fish" => Some(home.join(".config/fish/functions/fps.fish")),
        "zsh" => Some(home.join(".zshrc")),
        "bash" => Some(home.join(".bashrc")),
        _ => None,
    }
}

pub fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| std::path::Path::new(&s).file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "fish".to_string())
}
