/// Detects whether an actor name matches known agent/bot patterns.
pub struct AgentDetector {
    patterns: Vec<String>,
}

impl AgentDetector {
    /// Create a new detector with the given glob patterns.
    pub fn new(patterns: Vec<String>) -> Self {
        Self { patterns }
    }

    /// Check if the given actor string matches any agent pattern.
    /// Supports simple glob: `*` matches any characters.
    pub fn detect(&self, actor: &str) -> bool {
        self.patterns.iter().any(|p| glob_match(p, actor))
    }
}

/// Simple glob matching supporting only `*` as wildcard.
fn glob_match(pattern: &str, text: &str) -> bool {
    // Exact match
    if pattern == text {
        return true;
    }

    // No wildcards — exact match only
    if !pattern.contains('*') {
        return pattern == text;
    }

    let parts: Vec<&str> = pattern.split('*').collect();

    // Single * at the start: suffix match
    if parts.len() == 2 && parts[0].is_empty() {
        return text.ends_with(parts[1]);
    }

    // Single * at the end: prefix match
    if parts.len() == 2 && parts[1].is_empty() {
        return text.starts_with(parts[0]);
    }

    // General case: all parts must appear in order
    let mut remaining = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // First part must be a prefix
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == parts.len() - 1 {
            // Last part must be a suffix
            if !remaining.ends_with(part) {
                return false;
            }
            remaining = &remaining[..remaining.len() - part.len()];
        } else if let Some(pos) = remaining.find(part) {
            remaining = &remaining[pos + part.len()..];
        } else {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let d = AgentDetector::new(vec!["dependabot[bot]".to_string()]);
        assert!(d.detect("dependabot[bot]"));
        assert!(!d.detect("dependabot"));
    }

    #[test]
    fn suffix_wildcard() {
        let d = AgentDetector::new(vec!["*[bot]".to_string()]);
        assert!(d.detect("dependabot[bot]"));
        assert!(d.detect("renovate[bot]"));
        assert!(d.detect("[bot]"));
        assert!(!d.detect("dependabot"));
    }

    #[test]
    fn prefix_wildcard() {
        let d = AgentDetector::new(vec!["*-agent".to_string()]);
        assert!(d.detect("claude-agent"));
        assert!(d.detect("my-ci-agent"));
        assert!(!d.detect("agent-runner"));
    }

    #[test]
    fn no_match() {
        let d = AgentDetector::new(vec!["*[bot]".to_string(), "*-agent".to_string()]);
        assert!(!d.detect("alice"));
        assert!(!d.detect("human-user"));
    }

    #[test]
    fn multiple_patterns() {
        let d = AgentDetector::new(vec![
            "dependabot[bot]".to_string(),
            "*[bot]".to_string(),
            "*-agent".to_string(),
        ]);
        assert!(d.detect("dependabot[bot]"));
        assert!(d.detect("custom[bot]"));
        assert!(d.detect("my-agent"));
        assert!(!d.detect("alice"));
    }

    #[test]
    fn default_patterns_detect_common_bots() {
        let d = AgentDetector::new(crate::config::GitHubPollerConfig::default().agent_patterns);
        assert!(d.detect("dependabot[bot]"));
        assert!(d.detect("github-actions[bot]"));
        assert!(d.detect("renovate[bot]"));
        assert!(d.detect("custom[bot]"));
        assert!(d.detect("my-ci-agent"));
        assert!(!d.detect("alice"));
    }
}
