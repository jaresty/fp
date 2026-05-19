#[cfg(test)]
mod tests {
    #[test]
    fn profile_governs_profiles_path_contains_fp_config() {
        let path = crate::profile::profiles_path();
        let s = path.to_string_lossy();
        assert!(
            s.contains("fp") && s.contains("profiles.json"),
            "profile::profiles_path must return a path containing 'fp' and 'profiles.json': {}",
            s
        );
    }
}
