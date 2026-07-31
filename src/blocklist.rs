use crate::{
    domain::{DomainName, DomainRef},
    fetcher::Fetcher,
};
use ahash::AHashSet;
use std::{path::Path, sync::Arc};
use tracing::{info, warn};

pub struct Blocklist {
    domains: AHashSet<DomainName>,
}

impl Blocklist {
    pub async fn load_from_urls(urls: &[String]) -> Arc<Self> {
        let mut combined_list = String::new();
        let mut loaded_lists = 0;

        // Blocklists are fetched sequentially instead of concurrently.
        // Some lists can be massive (tens of MBs). Parsing them all at once 
        // risks an Out-Of-Memory (OOM) kill on constrained devices like Raspberry Pi.
        for (i, url) in urls.iter().enumerate() {
            let storage_file = format!("storage_{}.txt", i);
            let fetcher = Fetcher::new(url, Path::new(&storage_file));

            match fetcher.get_data().await {
                Ok(data) => {
                    info!("Loaded {} bytes from {}", data.len(), url);
                    combined_list.push_str(&data);
                    combined_list.push('\n');
                    loaded_lists += 1;
                }
                Err(e) => {
                    warn!(url = %url, error = %e, "Failed to load blocklist. Proceeding without it.");
                }
            }
        }

        if loaded_lists == 0 && !urls.is_empty() {
            warn!(
                "All blocklists failed to load. The DNS server will run, but no domains will be blocked."
            );
        }

        info!("Parsing blocklists...");
        let blocklist = Arc::new(Self::parse_list(&combined_list));
        info!("Blocklist loaded in memory.");
        blocklist
    }

    pub fn parse_list(raw_data: &str) -> Self {
        // Pre-allocate based on newline count. Fast O(N) scan.
        let lines_count = raw_data.as_bytes().iter().filter(|&&c| c == b'\n').count();
        let mut domains = AHashSet::with_capacity(lines_count);

        for line in raw_data.lines() {
            if let Some(d) = Self::parse_line(line) {
                domains.insert(d);
            }
        }

        domains.shrink_to_fit(); // Reclaim memory from rejected lines
        Self { domains }
    }

    fn parse_line(line: &str) -> Option<DomainName> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        // Handles both "domain.com" and "0.0.0.0 domain.com"
        let domain_str = line.split_whitespace().last()?;

        if domain_str == "localhost" || domain_str == "broadcasthost" {
            return None;
        }

        // Ignore invalid domains silently.
        // Blocklists are from 3rd parties and often contain garbage/unsupported syntax.
        // Crashing or logging every error here would spam I/O and halt the blocker.
        DomainName::try_from(domain_str).ok()
    }
}

impl Blocklist {
    pub fn is_blocked(&self, domain: DomainRef<'_>) -> bool {
        self.domains.contains(domain.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocklist_hit_miss() {
        let raw = "0.0.0.0 ad.com\n127.0.0.1 tracker.org\n# comment\nclean.com";
        let bl = Blocklist::parse_list(raw);

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
        let bl = Blocklist::parse_list(raw);
        assert!(!bl.is_blocked(DomainRef::parse("localhost").unwrap()));
    }
}
