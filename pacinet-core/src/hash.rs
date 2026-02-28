/// Content hash for policy identity comparison.
/// Uses SipHash (not cryptographic). Deterministic within a Rust version.
pub fn policy_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic() {
        let h1 = policy_hash("rules: []");
        let h2 = policy_hash("rules: []");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_different_content() {
        let h1 = policy_hash("rules: [a]");
        let h2 = policy_hash("rules: [b]");
        assert_ne!(h1, h2);
    }
}
