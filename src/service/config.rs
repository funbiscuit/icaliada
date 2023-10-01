use std::env;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use secrecy::{ExposeSecret, Secret};
use serde::Deserialize;

const DEFAULT_CONFIG_FILE: &str = "config";

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,

    /// All feeds
    pub feeds: Vec<FeedConfig>,
}

impl AppConfig {
    pub fn get_feed_by_token(&self, token: &str) -> Option<&FeedConfig> {
        //todo use hashmap
        self.feeds.iter().find(|feed| {
            feed.tokens.private.expose_secret() == token
                || feed.tokens.public.expose_secret() == token
        })
    }

    pub fn load() -> Result<Self, config::ConfigError> {
        let config_file = env::var("APP_CONFIG").unwrap_or_else(|_| DEFAULT_CONFIG_FILE.into());

        config::Config::builder()
            .add_source(config::File::with_name("config-default"))
            .add_source(config::File::with_name(&config_file).required(false))
            // Add in settings from environment variables (with a prefix of APP and '_' as separator)
            // E.g. `APP_SERVER_PORT=5001 would set `AppConfig.server.port`
            .add_source(config::Environment::with_prefix("APP").separator("_"))
            .build()?
            .try_deserialize()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    /// Host on which app should listen to
    pub host: String,

    /// Port on which app should listen to
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeedConfig {
    /// Name of this feed
    pub name: String,

    /// Tokens to access this feed
    pub tokens: TokensConfig,

    /// Port on which app should listen to
    pub calendars: Vec<CalendarConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TokensConfig {
    /// Token to access all information from calendar
    pub private: Secret<String>,

    /// Token to access free-busy information from calendar
    pub public: Secret<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CalendarConfig {
    /// Url of ical calendar
    pub url: Secret<String>,
}

impl PartialEq for CalendarConfig {
    fn eq(&self, other: &Self) -> bool {
        self.url.expose_secret() == other.url.expose_secret()
    }
}

impl Eq for CalendarConfig {}

impl Hash for CalendarConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url.expose_secret().hash(state)
    }
}
