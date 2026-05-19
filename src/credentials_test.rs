#[cfg(test)]
mod tests {
    #[test]
    fn credentials_module_governs_resolve_github_token_with() {
        let result = crate::credentials::resolve_github_token_with(Some("env-tok".to_string()), None);
        assert_eq!(result.unwrap(), "env-tok");
    }

    #[test]
    fn credentials_module_governs_resolve_github_token_error() {
        let err = crate::credentials::resolve_github_token_with(None, None).unwrap_err();
        assert!(err.to_string().contains("GITHUB_TOKEN"), "got: {}", err);
    }
}
