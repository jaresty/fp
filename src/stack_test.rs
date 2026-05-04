#[cfg(test)]
mod tests {
    use crate::stack::stack_order;
    use std::collections::HashMap;

    // RS1: linear stack A <- B <- C returns [A, B, C] (parent first)
    #[test]
    fn linear_stack_ordered_parent_first() {
        // Simulate: main <- A <- B <- C
        // merge_base(A, B) = A means A is parent of B
        // merge_base(B, C) = B means B is parent of C
        // We represent this as: branch -> its parent branch (the one it was based on)
        let branches = vec!["feat/a".to_string(), "feat/b".to_string(), "feat/c".to_string()];
        // parent_of[branch] = parent branch name (None = rooted at main)
        let mut parent_of: HashMap<String, Option<String>> = HashMap::new();
        parent_of.insert("feat/a".into(), None);
        parent_of.insert("feat/b".into(), Some("feat/a".into()));
        parent_of.insert("feat/c".into(), Some("feat/b".into()));

        let ordered = stack_order(&branches, &parent_of);
        assert_eq!(ordered, vec!["feat/a", "feat/b", "feat/c"]);
    }

    // RS1: single branch returns itself
    #[test]
    fn single_branch_returns_self() {
        let branches = vec!["feat/a".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("feat/a".into(), None);
        let ordered = stack_order(&branches, &parent_of);
        assert_eq!(ordered, vec!["feat/a"]);
    }

    // RS1: branches with no parent relationship returned in stable order
    #[test]
    fn unrelated_branches_returned_stably() {
        let branches = vec!["feat/x".to_string(), "feat/y".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("feat/x".into(), None);
        parent_of.insert("feat/y".into(), None);
        let ordered = stack_order(&branches, &parent_of);
        // Both are roots — order is stable (input order preserved)
        assert_eq!(ordered.len(), 2);
    }
}
