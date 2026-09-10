//! Source precedence, duplicate detection and dotenv registration rules.

use bui_env_loader::source::{MapSource, Source};
use bui_env_loader::{
    ConfigContext, ConfigError, EnvConfig, IssueKind, IssueSeverity, Loader,
    MaterializationContext, MaterializationMode, Materialize, SourceError,
};

#[derive(Debug)]
struct Config {
    host: String,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            host: context.get("APP_HOST")?,
        })
    }
}

impl Materialize for Config {
    type Output = String;

    fn materialize(&self, _context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        Some(self.host.clone())
    }
}

#[test]
fn explicit_values_win_over_every_other_source() {
    let loaded = Loader::new()
        .set("APP_HOST", "explicit")
        .source(MapSource::new("custom").with("APP_HOST", "custom"))
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.host, "explicit");
}

#[test]
fn custom_sources_win_over_dotenv_files() {
    let loaded = Loader::new()
        .source(MapSource::new("custom").with("APP_HOST", "custom"))
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.host, "custom");
}

#[test]
fn process_environment_sits_between_custom_sources_and_dotenv() {
    std::env::set_var("APP_HOST", "from-process");

    let via_process = Loader::new()
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .load::<Config>()
        .unwrap();
    assert_eq!(via_process.host, "from-process");

    let via_custom = Loader::new()
        .source(MapSource::new("custom").with("APP_HOST", "custom"))
        .load::<Config>()
        .unwrap();
    assert_eq!(via_custom.host, "custom");

    std::env::remove_var("APP_HOST");
}

#[test]
fn without_process_environment_ignores_process_variables() {
    std::env::set_var("APP_HOST", "from-process");

    let loaded = Loader::new()
        .without_process_environment()
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.host, "127.0.0.1");
    std::env::remove_var("APP_HOST");
}

#[test]
fn the_same_key_in_several_sources_is_reported_as_a_duplicate() {
    let materialized = Loader::new()
        .without_process_environment()
        .set("APP_HOST", "explicit")
        .source(MapSource::new("custom").with("APP_HOST", "custom"))
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .load::<Config>()
        .unwrap()
        .materialize(MaterializationMode::Warn)
        .unwrap();

    let duplicates: Vec<_> = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::DuplicateVariable)
        .collect();

    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].severity, IssueSeverity::Warning);
    assert_eq!(duplicates[0].variable.as_deref(), Some("APP_HOST"));
    let sources = duplicates[0].source.as_deref().expect("names the sources");
    assert!(sources.contains("explicit"), "{sources}");
    assert!(sources.contains("custom"), "{sources}");
    assert!(sources.contains(".env.test"), "{sources}");
}

#[test]
fn a_duplicate_inside_one_dotenv_file_survives_until_materialization() {
    let materialized = Loader::new()
        .without_process_environment()
        .add_dotenv("./tests/.env.test_duplicate")
        .load::<Config>()
        .unwrap()
        .materialize(MaterializationMode::Warn)
        .unwrap();

    assert_eq!(materialized.value, "second");

    let duplicates: Vec<_> = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::DuplicateVariable)
        .collect();
    assert_eq!(duplicates.len(), 1, "the last value in the file wins");
}

#[test]
fn optional_dotenv_files_are_skipped_when_absent() {
    let loaded = Loader::new()
        .set("APP_HOST", "fallback")
        .add_dotenv_optional("./tests/.env.test_non_existent")
        .prefix("APP_")
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.host, "fallback");
}

#[test]
fn earlier_dotenv_files_win_over_later_ones() {
    // APP_HOST differs between the two fixture files, proving that the first
    // registered dotenv file wins. The key is never set in the process
    // environment, so parallel tests cannot disturb this expectation.
    let loaded = Loader::new()
        .without_process_environment()
        .add_dotenv("./tests/.env.test")
        .add_dotenv("./tests/.env.test_override")
        .load::<Config>()
        .unwrap();

    assert_eq!(loaded.host, "127.0.0.1");
}

struct FailingSource;

impl Source for FailingSource {
    fn name(&self) -> &str {
        "failing"
    }

    fn get(&self, _key: &str) -> Result<Option<String>, SourceError> {
        Err(SourceError::new("source exploded"))
    }
}

#[test]
fn a_failing_source_stops_loading_with_a_source_error() {
    let error = Loader::new()
        .without_process_environment()
        .source(FailingSource)
        .load::<Config>()
        .unwrap_err();

    assert_eq!(error.kind(), bui_env_loader::ConfigErrorKind::Source);
    assert_eq!(error.source_name(), Some("failing"));
    assert_eq!(error.key(), Some("APP_HOST"));
}
