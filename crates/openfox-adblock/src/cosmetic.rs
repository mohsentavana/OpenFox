use std::collections::HashMap;

/// Cosmetic filtering, Brave-style: selectors to hide rather than requests to
/// block. Rules are stored per host suffix; an empty host key applies to all
/// pages. The engine emits one stylesheet per page load — collapse, never
/// leave holes.
pub struct CosmeticEngine {
    /// host suffix -> selectors
    keyed: HashMap<String, Vec<String>>,
    /// selectors applying to every page
    generic: Vec<String>,
}

impl CosmeticEngine {
    pub fn new() -> Self {
        CosmeticEngine { keyed: HashMap::new(), generic: Vec::new() }
    }

    /// `hosts` is the comma-separated host list from the rule; an empty list
    /// is the generic bucket.
    pub fn add(&mut self, hosts: Vec<String>, selector: String) {
        if hosts.is_empty() {
            if !self.generic.contains(&selector) {
                self.generic.push(selector);
            }
            return;
        }
        for host in hosts {
            let entry = self.keyed.entry(host).or_default();
            if !entry.contains(&selector) {
                entry.push(selector.clone());
            }
        }
    }

    /// Selectors to hide on a page, walking the host's label suffixes so a
    /// rule for `example.com` also fires on `news.example.com`.
    pub fn hide_for(&self, host: &str) -> Vec<String> {
        let mut out: Vec<String> = self.generic.clone();
        let mut labels: Vec<&str> = host.split('.').collect();
        while labels.len() > 1 {
            let suffix = labels.join(".");
            if let Some(selectors) = self.keyed.get(&suffix.to_ascii_lowercase()) {
                out.extend(selectors.iter().cloned());
            }
            labels.remove(0);
        }
        out
    }

    /// The stylesheet the engine injects into the page: one block, hidden
    /// with `!important`, so author styles cannot win the fight for space.
    pub fn stylesheet(&self, host: &str) -> String {
        let selectors = self.hide_for(host);
        if selectors.is_empty() {
            return String::new();
        }
        format!("{} {{ display: none !important; }}", selectors.join(", "))
    }

    pub fn len(&self) -> usize {
        self.generic.len() + self.keyed.values().map(Vec::len).sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_and_keyed_selectors() {
        let mut engine = CosmeticEngine::new();
        engine.add(vec![], ".ad-slot".into());
        engine.add(vec!["news.example.com".into()], ".sponsored-story".into());
        assert_eq!(engine.hide_for("news.example.com").len(), 2);
        assert_eq!(engine.hide_for("other.org").len(), 1);
        assert!(engine.stylesheet("news.example.com").contains("display: none !important"));
    }
}
