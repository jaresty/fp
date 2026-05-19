#[cfg(test)]
mod tests {
    #[test]
    fn upload_governs_parse_upload_token_finds_token() {
        let html = r#"{"uploadToken":"abc123"}"#;
        let result = crate::upload::parse_upload_token(html).unwrap();
        assert_eq!(result, "abc123", "upload::parse_upload_token must extract token from HTML");
    }

    #[test]
    fn upload_governs_inject_demo_section_appends_when_no_demo() {
        let body = "Some PR body";
        let result = crate::upload::inject_demo_section(body, &["https://example.com/img.png".to_string()]);
        assert!(result.contains("## Demo"), "upload::inject_demo_section must add Demo section");
        assert!(result.contains("https://example.com/img.png"), "upload::inject_demo_section must include URL");
    }

    #[test]
    fn upload_governs_parse_upload_policy_response_extracts_fields() {
        let json = r#"{"upload_url":"https://s3.example.com","asset":{"id":42,"href":"https://github.com/assets/42"},"asset_upload_authenticity_token":"tok","form":{"key":"val"}}"#;
        let policy = crate::upload::parse_upload_policy_response(json).unwrap();
        assert_eq!(policy.asset_id, 42, "upload::parse_upload_policy_response must extract asset id");
        assert_eq!(policy.upload_url, "https://s3.example.com");
    }
}
