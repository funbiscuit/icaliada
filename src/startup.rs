use std::fmt::{Debug, Formatter};
use std::future::IntoFuture;
use std::pin::Pin;

use futures::Future;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::routes;
use crate::service::config::AppConfig;
use crate::service::feeds::FeedService;

pub struct Application {
    port: u16,
    server: Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>>,
    shutdown_hook: Option<oneshot::Sender<()>>,
}

impl Debug for Application {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Application")
            .field("port", &self.port)
            .finish()
    }
}

impl Application {
    pub async fn build(config: AppConfig) -> anyhow::Result<Self> {
        tracing::info!("Using config: {:?}", config);

        let listener = TcpListener::bind((config.server.host.as_str(), config.server.port)).await?;
        let port = listener.local_addr().unwrap().port();

        tracing::info!("Listening on port {}", port);

        let feed_service = FeedService::new(&config);

        let router = routes::create_router(config, feed_service);

        let serve = axum::serve(listener, router.into_make_service());
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let graceful = serve.with_graceful_shutdown(async {
            rx.await.ok();
        });

        Ok(Self {
            port,
            server: Box::pin(graceful.into_future()),
            shutdown_hook: Some(tx),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn shutdown_hook(&mut self) -> Option<oneshot::Sender<()>> {
        self.shutdown_hook.take()
    }

    pub async fn wait_finish(self) -> std::io::Result<()> {
        self.server.await
    }
}
