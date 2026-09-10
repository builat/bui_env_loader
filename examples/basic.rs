use std::fmt::Debug;

use bui_env_loader::{
    Computed, ConfigContext, ConfigError, EnvConfig, Event, Lazy, LazyOptional, Loader,
    MaterializationContext, MaterializationMode, Materialize, ResolveSession,
};

/// The runtime form of the application configuration.
///
/// Eager fields contain normal Rust values. Deferred fields contain recipes
/// that can be executed repeatedly.
#[derive(Debug)]
struct AppConfig {
    host: String,
    port: u16,
    token: Option<String>,
    log_level: Lazy<String>,
    metrics_endpoint: LazyOptional<String>,
    public_url: Computed<String>,
}

impl EnvConfig for AppConfig {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            // The target field types tell Rust which FromStr implementation to
            // use. A malformed APP_PORT therefore cannot become a String or be
            // silently replaced with zero.
            host: context.get("APP_HOST")?,
            port: context.get("APP_PORT")?,
            token: context.get_optional("APP_TOKEN")?,

            // These calls validate and register the key now, but do not read
            // its value until Lazy::get or materialize is invoked.
            log_level: context.lazy_get("APP_LOG_LEVEL")?,
            metrics_endpoint: context.lazy_get_optional("APP_METRICS_ENDPOINT")?,

            // A computed field receives the active resolution session. During
            // materialization this is the same session used by other fields,
            // so related reads form a consistent snapshot.
            public_url: context.get_computed("public_url", |session: &mut ResolveSession| {
                let host = session.get::<String>("APP_HOST")?;
                let port = session.get::<u16>("APP_PORT")?;
                Ok(format!("http://{host}:{port}"))
            }),
        })
    }
}

/// The immutable, fully resolved form returned by `materialize`.
#[derive(Debug, PartialEq, Eq)]
struct AppConfigSnapshot {
    host: String,
    port: u16,
    token: Option<String>,
    log_level: String,
    metrics_endpoint: Option<String>,
    public_url: String,
}

impl Materialize for AppConfig {
    type Output = AppConfigSnapshot;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        // Resolve every deferred field before checking the results. Avoiding
        // early `?` is what lets Strict mode return an aggregate report.
        let log_level = context.lazy("log_level", &self.log_level);
        let metrics_endpoint = context.lazy_optional("metrics_endpoint", &self.metrics_endpoint);
        let public_url = context.computed("public_url", &self.public_url);

        match (log_level, metrics_endpoint, public_url) {
            (Some(log_level), Some(metrics_endpoint), Some(public_url)) => {
                Some(AppConfigSnapshot {
                    // Materialize takes `&self` so it can be called repeatedly.
                    // Eager owned fields therefore need to be cloned.
                    host: self.host.clone(),
                    port: self.port,
                    token: self.token.clone(),
                    log_level,
                    metrics_endpoint,
                    public_url,
                })
            }
            _ => None,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = Loader::new()
        // Explicit values have the highest priority. They also make this
        // example deterministic even when no local .env file exists.
        .set("APP_HOST", "127.0.0.1")
        .set("APP_PORT", "8080")
        .set("APP_LOG_LEVEL", "debug")
        .add_dotenv_optional(".env")
        .prefix("APP_")
        .reporter(|event: &Event<'_>| {
            eprintln!(
                "{:?} {:?}: {} (field={:?}, key={:?}, source={:?})",
                event.level, event.kind, event.message, event.field, event.key, event.source
            );
        })
        .load::<AppConfig>()?;

    // Each direct call performs a new lookup.
    println!("current log level: {}", loaded.log_level.get()?);

    // Notify logs absent optional variables but still returns a snapshot.
    let materialized = loaded.materialize(MaterializationMode::Notify)?;
    println!("snapshot: {:#?}", materialized.value);

    Ok(())
}
