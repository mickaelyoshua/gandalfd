use std::path::{Path, PathBuf};

use reqwest::Client;
use tokio::fs;

pub struct Fetcher {
    cache_path: PathBuf,
    url: String,
}

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("network or request error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io error accessing cache: {0}")]
    Io(#[from] std::io::Error),
}

impl Fetcher {
    pub fn new(url: &str, cache_path: &Path) -> Self {
        Self {
            cache_path: cache_path.to_path_buf(),
            url: url.to_string(),
        }
    }

    pub async fn get_data(&self) -> Result<String, FetchError> {
        if let Ok(data) = fs::read_to_string(&self.cache_path).await {
            return Ok(data);
        }

        self.force_update().await?;
        let data = fs::read_to_string(&self.cache_path).await?;
        Ok(data)
    }

    pub async fn force_update(&self) -> Result<(), FetchError> {
        let client = Client::new();
        let bytes = client.get(&self.url).send().await?.bytes().await?;

        // Atomic write: Avoids corrupted file on electric failure.
        // Crucial for Raspberry Pi reliability where power drops are frequent during writes.
        let tmp_path = self.cache_path.with_extension("tmp");
        fs::write(&tmp_path, &bytes).await?;
        fs::rename(&tmp_path, &self.cache_path).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fetcher_reads_from_cache() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("hosts.txt");

        fs::write(&cache, "0.0.0.0 fake.com").await.unwrap();

        let fetcher = Fetcher::new("http://invalid", &cache);
        let data = fetcher.get_data().await.unwrap();

        assert_eq!(data, "0.0.0.0 fake.com");
    }
}
