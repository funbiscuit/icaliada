use axum::{routing::get, Extension, Router};

use tower::ServiceBuilder;

use crate::config::AppConfig;
use crate::routes::feeds;
use crate::service::feeds::FeedService;

pub fn create_router(config: AppConfig, feed_service: FeedService) -> Router {
    Router::new()
        .route("/events", get(feeds::get_events_feed))
        .route("/feeds/feed.html", get(feeds::get_html_feed))
        .layer(
            ServiceBuilder::new()
                .layer(Extension(config))
                .layer(Extension(feed_service)),
        )
}
