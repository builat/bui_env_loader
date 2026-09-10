//! The `env_config!` macro: generating both config forms at once.
//!
//! Writing `EnvConfig` and `Materialize` by hand (see `basic.rs`) is fully
//! supported, but the declarative macro removes the boilerplate: one block
//! describes every field and generates the runtime struct, the snapshot
//! struct, and both trait implementations.
//!
//! The reader used after `=` decides the field shape:
//!   get / get_or       -> T
//!   get_optional       -> Option<T>
//!   lazy_get           -> Lazy<T>
//!   lazy_get_optional  -> LazyOptional<T>
//!   get_computed       -> Computed<T>
//!
//! Run with: cargo run --example macro_config

use bui_env_loader::{env_config, Loader, MaterializationMode, ResolveSession};

env_config! {
    pub config AppConfig => AppConfigSnapshot {
        host: String =
            get("APP_HOST");

        port: u16 =
            get("APP_PORT");

        // Option<String> in both runtime and snapshot forms.
        token: String =
            get_optional("APP_TOKEN");

        // Falls back to 4 and records a DefaultUsed notice.
        workers: usize =
            get_or("APP_WORKERS", 4);

        // Lazy<String>: re-read on every .get().
        log_level: String =
            lazy_get("APP_LOG_LEVEL");

        // LazyOptional<String>.
        metrics_endpoint: String =
            lazy_get_optional("APP_METRICS_ENDPOINT");

        // Computed<String>: the closure runs on every .get().
        public_url: String =
            get_computed(
                "public_url",
                |session: &mut ResolveSession| {
                    let host =
                        session.get::<String>("APP_HOST")?;

                    let port =
                        session.get::<u16>("APP_PORT")?;

                    Ok(format!("http://{host}:{port}"))
                }
            );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = Loader::new()
        .without_process_environment()
        .set("APP_HOST", "127.0.0.1")
        .set("APP_PORT", "8080")
        .set("APP_LOG_LEVEL", "debug")
        .add_dotenv_optional("examples/.env.example_local")
        .prefix("APP_")
        .load::<AppConfig>()?;

    // Runtime form: deferred fields are recipes, resolved on demand.
    println!("host:      {}", loaded.host);
    println!("port:      {}", loaded.port);
    println!("token:     {:?}", loaded.token);
    println!("workers:   {}", loaded.workers);
    println!("log level: {}", loaded.log_level.get()?);
    println!("url:       {}", loaded.public_url.get()?);

    // Snapshot form: everything resolved once, fully owned. (The generated
    // snapshot struct does not derive Debug, so fields are printed directly.)
    let materialized = loaded.materialize(MaterializationMode::Notify)?;
    let snapshot = materialized.value;
    println!(
        "snapshot:  {}:{} workers={} url={}",
        snapshot.host, snapshot.port, snapshot.workers, snapshot.public_url
    );
    println!("issues:    {}", materialized.report.issues().len());

    Ok(())
}
