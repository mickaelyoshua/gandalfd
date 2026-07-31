use gandalfd::{app::GandalfApp, observability, settings::AppSettings};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init();

    let config = AppSettings::load();
    info!(?config, "Gandalf DNS Blocker starting...");

    GandalfApp::build(config).await?.run().await
}
