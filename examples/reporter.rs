//! Reporters: bridging library events to application logging.
//!
//! The library never prints anything itself. Every diagnostic is delivered as
//! a structured `Event` to a `Reporter`, which can be a closure (as here) or
//! any type implementing the trait. Events carry names and source metadata,
//! never raw values, so piping them into logs cannot leak secrets.
//!
//! Run with: cargo run --example reporter

use std::sync::{Arc, Mutex};

use bui_env_loader::{
    Computed, ConfigContext, ConfigError, EnvConfig, Event, Level, Loader, MaterializationMode,
};

#[derive(Debug)]
struct Config {
    port: u16,
    summary: Computed<String>,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            port: context.get("APP_PORT")?,
            summary: context.get_computed("summary", |session| {
                let port: u16 = session.get("APP_PORT")?;
                Ok(format!("serving on {port}"))
            }),
        })
    }
}

impl bui_env_loader::Materialize for Config {
    type Output = String;

    fn materialize(
        &self,
        context: &mut bui_env_loader::MaterializationContext<'_>,
    ) -> Option<Self::Output> {
        context.computed("summary", &self.summary)
    }
}

/// A custom reporter with state: counts events per level. Cloning shares the
/// counters, so the loader can own one copy while `main` reads the totals.
#[derive(Clone, Default)]
struct Statistics {
    counts: Arc<Mutex<Vec<(Level, String)>>>,
}

impl bui_env_loader::Reporter for Statistics {
    fn report(&self, event: &Event<'_>) {
        self.counts
            .lock()
            .unwrap()
            .push((event.level, event.message.to_owned()));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stats = Statistics::default();

    let loaded = Loader::new()
        .without_process_environment()
        .set("APP_PORT", "8080")
        .reporter(stats.clone())
        .load::<Config>()?;

    loaded.materialize(MaterializationMode::Normal)?;

    println!("config port: {}", loaded.port);

    let events = stats.counts.lock().unwrap();
    println!("{} events were emitted in total:", events.len());
    for (level, message) in events.iter() {
        println!("  {level:?}  {message}");
    }

    // The same events can be filtered, formatted, or forwarded to `log`/
    // `tracing` without the library knowing about either ecosystem.
    let errors = events
        .iter()
        .filter(|(level, _)| *level == Level::Error)
        .count();
    println!("error-level events: {errors}");

    Ok(())
}
