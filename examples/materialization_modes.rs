//! Materialization: turning the runtime config into an immutable snapshot.
//!
//! The four modes differ only in how discrepancies are logged and tolerated:
//!
//! - `Normal`: silent unless a fatal error occurs;
//! - `Notify`: additionally mentions missing optional variables at Info;
//! - `Warn`:  reports every discrepancy but still returns the snapshot;
//! - `Strict`: reports every discrepancy and rejects the snapshot if any
//!   issue exists, returning an aggregate report instead of the first error.
//!
//! Run with: cargo run --example materialization_modes

use bui_env_loader::{
    ConfigContext, ConfigError, EnvConfig, Lazy, LazyOptional, Loader, MaterializationContext,
    MaterializationMode, Materialize,
};

#[derive(Debug)]
struct Config {
    port: u16,
    log_level: Lazy<String>,
    token: LazyOptional<String>,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            port: context.get("APP_PORT")?,
            log_level: context.lazy_get("APP_LOG_LEVEL")?,
            token: context.lazy_get_optional("APP_TOKEN")?,
        })
    }
}

#[derive(Debug)]
struct Snapshot {
    port: u16,
    log_level: String,
    token: Option<String>,
}

impl Materialize for Config {
    type Output = Snapshot;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        // Resolve every deferred field first; no early `?`, so Strict mode can
        // aggregate every problem into one report.
        let log_level = context.lazy("log_level", &self.log_level);
        let token = context.lazy_optional("token", &self.token);

        match (log_level, token) {
            (Some(log_level), Some(token)) => Some(Snapshot {
                port: self.port,
                log_level,
                token,
            }),
            _ => None,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = Loader::new()
        .without_process_environment()
        .set("APP_PORT", "8080")
        .set("APP_LOG_LEVEL", "debug")
        // APP_TOKEN is deliberately absent: a Notice, not an error.
        .load::<Config>()?;

    for mode in [
        MaterializationMode::Normal,
        MaterializationMode::Notify,
        MaterializationMode::Warn,
        MaterializationMode::Strict,
    ] {
        print!("mode {mode:?}: ");
        match loaded.materialize(mode) {
            Ok(snapshot) => println!(
                "accepted, {} issue(s), port={}, log_level={}, token={:?}",
                snapshot.report.issues().len(),
                snapshot.value.port,
                snapshot.value.log_level,
                snapshot.value.token
            ),
            Err(rejection) => {
                println!("rejected with {} issue(s)", rejection.report.issues().len())
            }
        }
    }

    // Fatal errors reject the snapshot in every mode. Here the lazy field's
    // value cannot be parsed into u16 during materialization.
    #[derive(Debug)]
    struct Broken {
        value: Lazy<u16>,
    }

    impl EnvConfig for Broken {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            Ok(Self {
                value: context.lazy_get("APP_NUMBER")?,
            })
        }
    }

    impl Materialize for Broken {
        type Output = u16;

        fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
            context.lazy("value", &self.value)
        }
    }

    let loaded = Loader::new()
        .without_process_environment()
        .set("APP_NUMBER", "not-a-number")
        .load::<Broken>()?;

    let rejection = loaded.materialize(MaterializationMode::Normal).unwrap_err();
    println!(
        "fatal case: rejected, has_errors={}, first issue kind={:?}",
        rejection.report.has_errors(),
        rejection.report.issues()[0].kind
    );

    Ok(())
}
