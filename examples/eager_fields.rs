//! Eager fields: required, optional and defaulted values read immediately.
//!
//! Run with: cargo run --example eager_fields

use std::net::SocketAddr;

use bui_env_loader::{ConfigContext, ConfigError, EnvConfig, Loader};

#[derive(Debug)]
struct Config {
    // `get` requires the variable and parses it with the field's `FromStr`.
    // A missing DATABASE_URL or a malformed PORT fails `load` immediately.
    database_url: String,
    port: u16,
    // Any std type with a `FromStr` works, including more exotic ones.
    listen_addr: SocketAddr,
    debug: bool,
    ratio: f64,

    // `get_optional` returns Option<T>: absence is not an error.
    api_token: Option<String>,

    // `get_or` falls back to a typed default when the key is absent.
    // The fallback is recorded as a `DefaultUsed` notice in the report.
    workers: usize,
    timeout_secs: u64,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: context.get("APP_DATABASE_URL")?,
            port: context.get("APP_PORT")?,
            listen_addr: context.get("APP_LISTEN_ADDR")?,
            debug: context.get("APP_DEBUG")?,
            ratio: context.get("APP_RATIO")?,

            api_token: context.get_optional("APP_API_TOKEN")?,

            workers: context.get_or("APP_WORKERS", 4)?,
            timeout_secs: context.get_or("APP_TIMEOUT_SECS", 30)?,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = Loader::new()
        .without_process_environment()
        // Explicit values are the highest-priority source, which keeps this
        // example deterministic no matter where it runs.
        .set("APP_DATABASE_URL", "postgres://localhost/app")
        .set("APP_PORT", "8080")
        .set("APP_LISTEN_ADDR", "0.0.0.0:8080")
        .set("APP_DEBUG", "true")
        .set("APP_RATIO", "0.75")
        // APP_API_TOKEN is intentionally absent -> None.
        // APP_WORKERS is intentionally absent -> default of 4.
        .set("APP_TIMEOUT_SECS", "10")
        .load::<Config>()?;

    // `Loaded<T>` derefs to `T`, so fields are read directly.
    println!("database_url: {}", loaded.database_url);
    println!("port:         {}", loaded.port);
    println!("listen_addr:  {}", loaded.listen_addr);
    println!("debug:        {}", loaded.debug);
    println!("ratio:        {}", loaded.ratio);
    println!("api_token:    {:?}", loaded.api_token);
    println!("workers:      {} (default)", loaded.workers);
    println!("timeout_secs: {}", loaded.timeout_secs);

    Ok(())
}
