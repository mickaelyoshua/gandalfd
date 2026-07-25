use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init() {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info!("Observability initialized");
}
