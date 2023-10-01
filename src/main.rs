use icaliada::config::AppConfig;
use icaliada::Application;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Populate environment from .env
    let dot_env_missing = dotenv::dotenv().is_err();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "icaliada=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if dot_env_missing {
        tracing::warn!(".env file is missing");
    }

    tracing::warn!("Warn logging is enabled");
    tracing::info!("Info logging is enabled");
    tracing::debug!("Debug logging is enabled");
    tracing::trace!("Trace logging is enabled");

    let config = AppConfig::load()?;

    let application = Application::build(config).await?;
    application.wait_finish().await?;

    Ok(())
}
