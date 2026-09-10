//! Deferred fields: `Lazy`, `LazyOptional` and `Computed`.
//!
//! Lazy values are looked up again on every call, which lets long-running
//! processes pick up changed sources without reloading. Computed fields run a
//! closure on every call and can combine several variables.
//!
//! Run with: cargo run --example deferred_fields

use std::sync::Arc;

use bui_env_loader::source::MapSource;
use bui_env_loader::{
    Computed, ConfigContext, ConfigError, EnvConfig, Lazy, LazyOptional, Loader, ResolveSession,
};

#[derive(Debug)]
struct Config {
    log_level: Lazy<String>,
    feature_flags: LazyOptional<String>,
    public_url: Computed<String>,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            // The key is validated now, but the value is not read until get().
            log_level: context.lazy_get("APP_LOG_LEVEL")?,

            feature_flags: context.lazy_get_optional("APP_FEATURE_FLAGS")?,

            // The closure receives the active ResolveSession and can read any
            // variable, even ones no other field declares.
            public_url: context.get_computed("public_url", |session: &mut ResolveSession| {
                let host: String = session.get("APP_HOST")?;
                let port: u16 = session.get("APP_PORT")?;
                Ok(format!("https://{host}:{port}"))
            }),
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A mutable map shared with the loader via a custom source. The example
    // mutates it between reads to show that Lazy really re-reads the source.
    let map = Arc::new(std::sync::Mutex::new(MapSource::new("live")));
    {
        let mut guard = map.lock().unwrap();
        guard.insert("APP_LOG_LEVEL", "info");
        guard.insert("APP_HOST", "example.test");
        guard.insert("APP_PORT", "443");
    }

    struct SharedMap(std::sync::Arc<std::sync::Mutex<MapSource>>);

    impl bui_env_loader::source::Source for SharedMap {
        fn name(&self) -> &str {
            "live-map"
        }

        fn get(&self, key: &str) -> Result<Option<String>, bui_env_loader::SourceError> {
            Ok(self.0.lock().unwrap().get(key).unwrap())
        }

        fn keys(&self) -> Result<Option<Vec<String>>, bui_env_loader::SourceError> {
            Ok(self.0.lock().unwrap().keys().unwrap())
        }
    }

    let loaded = Loader::new()
        .without_process_environment()
        .source(SharedMap(map.clone()))
        .load::<Config>()?;

    println!("log level:  {}", loaded.log_level.get()?);
    println!("url:        {}", loaded.public_url.get()?);
    println!(
        "flags:      {:?}",
        loaded.feature_flags.lazy_get_optional()?
    );

    // Simulate an operator changing the source while the process runs.
    map.lock().unwrap().insert("APP_LOG_LEVEL", "warn");
    map.lock().unwrap().insert("APP_PORT", "8443");

    // The very same handles observe the new values.
    println!("log level:  {}", loaded.log_level.get()?);
    println!("url:        {}", loaded.public_url.get()?);

    Ok(())
}
