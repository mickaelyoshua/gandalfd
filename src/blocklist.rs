use crate::domain::{Blocklist, DomainName, DomainRef};
use ahash::AHashSet;

pub struct OptimizedBlocklist {
    domains: AHashSet<DomainName>,
}

impl OptimizedBlocklist {
    pub fn parse_list(raw_data: &str) -> Self {
        // Pre-allocate based on newline count. Fast O(N) scan.
        let lines_count = raw_data.as_bytes().iter().filter(|&&c| c == b'\n').count();
        let mut domains = AHashSet::with_capacity(lines_count);

        for line in raw_data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handles both "domain.com" and "0.0.0.0 domain.com"
            if let Some(domain_str) = line.split_whitespace().last() {
                if domain_str == "localhost" || domain_str == "broadcasthost" {
                    continue;
                }

                // Ignore invalid domains silently.
                // Blocklists are from 3rd parties and often contain garbage/unsupported syntax.
                // Crashing or logging every error here would spam I/O and halt the blocker.
                if let Ok(d) = DomainName::try_from(domain_str) {
                    domains.insert(d);
                }
            }
        }

        domains.shrink_to_fit(); // Reclaim memory from rejected lines
        Self { domains }
    }
}

impl Blocklist for OptimizedBlocklist {
    fn is_blocked(&self, domain: DomainRef<'_>) -> bool {
        self.domains.contains(domain.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocklist_hit_miss() {
        let raw = "0.0.0.0 ad.com\n127.0.0.1 tracker.org\n# comment\nclean.com";
        let bl = OptimizedBlocklist::parse_list(raw);

        let blocked1 = DomainRef::parse("ad.com").unwrap();
        let blocked2 = DomainRef::parse("tracker.org").unwrap();
        let clean = DomainRef::parse("safe.com").unwrap(); // not in list

        assert!(bl.is_blocked(blocked1));
        assert!(bl.is_blocked(blocked2));
        assert!(!bl.is_blocked(clean));
    }

    #[test]
    fn ignores_localhost() {
        let raw = "0.0.0.0 localhost\n127.0.0.1 broadcasthost";
        let bl = OptimizedBlocklist::parse_list(raw);
        assert!(!bl.is_blocked(DomainRef::parse("localhost").unwrap()));
    }
}
