use std::collections::HashMap;

pub fn stack_order(branches: &[String], parent_of: &HashMap<String, Option<String>>) -> Vec<String> {
    let mut children: HashMap<Option<&str>, Vec<&str>> = HashMap::new();
    for branch in branches {
        let parent = parent_of.get(branch).and_then(|p| p.as_deref());
        children.entry(parent).or_default().push(branch.as_str());
    }

    let mut result = Vec::with_capacity(branches.len());
    let mut queue: Vec<&str> = children.remove(&None).unwrap_or_default();
    // stable: preserve input order within same level
    queue.sort_by_key(|b| branches.iter().position(|x| x == b));

    while !queue.is_empty() {
        let mut next_level: Vec<&str> = Vec::new();
        for b in &queue {
            result.push(b.to_string());
            if let Some(mut kids) = children.remove(&Some(b)) {
                kids.sort_by_key(|k| branches.iter().position(|x| x == k));
                next_level.extend(kids);
            }
        }
        queue = next_level;
    }
    result
}
