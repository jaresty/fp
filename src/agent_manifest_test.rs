#[cfg(test)]
mod tests {
    #[test]
    fn agent_module_governs_manifest_with_prs() {
        use crate::store::PrCache;
        let prs = vec![PrCache { number: 1, title: "t".into(), branch: "b".into(), base: "main".into() }];
        let m = crate::agent::agent_context_manifest_with_prs(&prs);
        assert!(m["tracked_prs"].is_array(), "got: {}", m);
    }
}
