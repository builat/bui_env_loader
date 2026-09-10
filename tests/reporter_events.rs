//! The reporter sees the documented events during load and materialization.

use std::sync::{Arc, Mutex};

use bui_env_loader::source::MapSource;
use bui_env_loader::{
    Computed, ConfigContext, ConfigError, EnvConfig, Event, EventKind, Lazy, Level, Loader,
    MaterializationContext, MaterializationMode, Materialize, Reporter,
};

/// Cloning shares one event log, so the loader can own a copy while the test
/// keeps another for inspection.
#[derive(Clone, Default)]
struct Collector {
    events: Arc<Mutex<Vec<(Level, EventKind, String)>>>,
}

impl Collector {
    fn levels(&self) -> Vec<(Level, EventKind)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|(level, kind, _)| (*level, *kind))
            .collect()
    }
}

impl Reporter for Collector {
    fn report(&self, event: &Event<'_>) {
        self.events
            .lock()
            .unwrap()
            .push((event.level, event.kind, event.message.to_owned()));
    }
}

struct Config {
    port: u16,
    label: Lazy<String>,
    url: Computed<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    port: u16,
    label: String,
    url: String,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            port: context.get("APP_PORT")?,
            label: context.lazy_get("APP_LABEL")?,
            url: context.get_computed("url", |session| {
                let port: u16 = session.get("APP_PORT")?;
                Ok(format!("http://localhost:{port}"))
            }),
        })
    }
}

impl Materialize for Config {
    type Output = Snapshot;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        Some(Snapshot {
            port: self.port,
            label: context.lazy("label", &self.label)?,
            url: context.computed("url", &self.url)?,
        })
    }
}

#[test]
fn load_and_materialize_emit_their_lifecycle_events() {
    let collector = Collector::default();

    let loaded = Loader::new()
        .without_process_environment()
        .source(
            MapSource::new("test")
                .with("APP_PORT", "8080")
                .with("APP_LABEL", "dev"),
        )
        .reporter(collector.clone())
        .load::<Config>()
        .unwrap();

    let levels = collector.levels();
    for expected in [
        (Level::Info, EventKind::LoadStarted),
        (Level::Trace, EventKind::VariableLookup),
        (Level::Debug, EventKind::VariableResolved),
        (Level::Info, EventKind::LoadFinished),
    ] {
        assert!(
            levels.contains(&expected),
            "missing {expected:?} during load in {levels:?}"
        );
    }

    loaded.materialize(MaterializationMode::Normal).unwrap();

    let levels = collector.levels();
    for expected in [
        (Level::Info, EventKind::MaterializationStarted),
        (Level::Trace, EventKind::ComputationStarted),
        (Level::Debug, EventKind::ComputationFinished),
        (Level::Info, EventKind::MaterializationFinished),
    ] {
        assert!(
            levels.contains(&expected),
            "missing {expected:?} during materialization in {levels:?}"
        );
    }
}

#[test]
fn dotenv_loading_is_reported_once_per_file() {
    let collector = Collector::default();

    Loader::new()
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .reporter(collector.clone())
        .load::<Config>()
        .unwrap();

    let dotenv_events = collector
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, kind, _)| *kind == EventKind::DotenvLoaded)
        .count();
    assert_eq!(dotenv_events, 1);
}

#[test]
fn load_errors_are_reported_at_error_level() {
    let collector = Collector::default();

    let result = Loader::new()
        .without_process_environment()
        .reporter(collector.clone())
        .load::<Config>();

    assert!(result.is_err());

    let errors = collector
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|(level, kind, _)| *level == Level::Error && *kind == EventKind::Issue)
        .count();
    assert!(errors > 0, "the missing variable must be reported");
}
