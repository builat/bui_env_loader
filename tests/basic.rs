use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bui_env_loader::source::{MapSource, Source};
use bui_env_loader::{
    Computed, ConfigContext, ConfigError, EnvConfig, IssueKind, Lazy, LazyOptional, Loader,
    MaterializationContext, MaterializationMode, Materialize, SourceError,
};

#[test]
fn eager_get_is_strongly_typed_and_optional_is_explicit() {
    struct Config {
        port: u16,
        token: Option<String>,
    }

    impl EnvConfig for Config {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            Ok(Self {
                port: context.get("APP_PORT")?,
                token: context.get_optional("APP_TOKEN")?,
            })
        }
    }

    let source = MapSource::new("test").with("APP_PORT", "8080");
    let loaded = Loader::new()
        .without_process_environment()
        .source(source)
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.port, 8080);
    assert_eq!(loaded.token, None);
}

#[derive(Clone)]
struct CountingSource {
    calls: Arc<AtomicUsize>,
}

impl Source for CountingSource {
    fn name(&self) -> &str {
        "counter"
    }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        if key != "APP_COUNTER" {
            return Ok(None);
        }

        let value = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Some(value.to_string()))
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(vec!["APP_COUNTER".to_owned()]))
    }
}

#[test]
fn lazy_get_reads_the_source_on_every_call() {
    struct Config {
        counter: Lazy<usize>,
    }

    impl EnvConfig for Config {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            Ok(Self {
                counter: context.lazy_get("APP_COUNTER")?,
            })
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let loaded = Loader::new()
        .without_process_environment()
        .source(CountingSource {
            calls: calls.clone(),
        })
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.counter.get().unwrap(), 1);
    assert_eq!(loaded.counter.get().unwrap(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn computed_closure_runs_on_every_call() {
    struct Config {
        value: Computed<usize>,
    }

    impl EnvConfig for Config {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            let calls = Arc::new(AtomicUsize::new(0));
            Ok(Self {
                value: context.get_computed("value", move |_| {
                    Ok(calls.fetch_add(1, Ordering::SeqCst) + 1)
                }),
            })
        }
    }

    let loaded = Loader::new()
        .without_process_environment()
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.value.get().unwrap(), 1);
    assert_eq!(loaded.value.get().unwrap(), 2);
}

struct DeferredConfig {
    eager_optional: Option<String>,
    number: Lazy<u16>,
    lazy_optional: LazyOptional<String>,
    label: Computed<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct DeferredSnapshot {
    eager_optional: Option<String>,
    number: u16,
    lazy_optional: Option<String>,
    label: String,
}

impl EnvConfig for DeferredConfig {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            eager_optional: context.get_optional("APP_EAGER_OPTIONAL")?,
            number: context.lazy_get("APP_NUMBER")?,
            lazy_optional: context.lazy_get_optional("APP_LAZY_OPTIONAL")?,
            label: context.get_computed("label", |session| {
                let number = session.get::<u16>("APP_NUMBER")?;
                Ok(format!("number-{number}"))
            }),
        })
    }
}

impl Materialize for DeferredConfig {
    type Output = DeferredSnapshot;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        let number = context.lazy("number", &self.number);
        let lazy_optional = context.lazy_optional("lazy_optional", &self.lazy_optional);
        let label = context.computed("label", &self.label);

        match (number, lazy_optional, label) {
            (Some(number), Some(lazy_optional), Some(label)) => Some(DeferredSnapshot {
                eager_optional: self.eager_optional.clone(),
                number,
                lazy_optional,
                label,
            }),
            _ => None,
        }
    }
}

fn deferred_config() -> bui_env_loader::Loaded<DeferredConfig> {
    Loader::new()
        .without_process_environment()
        .source(MapSource::new("test").with("APP_NUMBER", "7"))
        .prefix("APP_")
        .load::<DeferredConfig>()
        .unwrap()
}

#[test]
fn notify_allows_missing_optional_fields_and_returns_report() {
    let materialized = deferred_config()
        .materialize(MaterializationMode::Notify)
        .unwrap();

    assert_eq!(materialized.value.number, 7);
    assert_eq!(materialized.value.label, "number-7");
    assert_eq!(materialized.value.eager_optional, None);
    assert_eq!(materialized.value.lazy_optional, None);

    let missing_count = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::MissingOptional)
        .count();
    assert_eq!(missing_count, 2);
}

#[test]
fn strict_rejects_even_non_fatal_discrepancies() {
    let error = deferred_config()
        .materialize(MaterializationMode::Strict)
        .unwrap_err();

    assert!(!error.report.is_clean());
    assert!(!error.report.has_errors());
}
