//! Remaining public API surface: the `Loaded` handle, key validation,
//! defaults, unknown-variable auditing and the materialization guardrails.

use bui_env_loader::source::MapSource;
use bui_env_loader::{
    ConfigContext, ConfigError, ConfigErrorKind, EnvConfig, IssueKind, IssueSeverity, Lazy, Loader,
    MaterializationContext, MaterializationError, MaterializationMode, Materialize,
};

#[derive(Debug)]
struct Config {
    port: u16,
    timeout: u32,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            port: context.get("APP_PORT")?,
            timeout: context.get_or("APP_TIMEOUT", 30)?,
        })
    }
}

impl Materialize for Config {
    type Output = (u16, u32);

    fn materialize(&self, _context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        Some((self.port, self.timeout))
    }
}

fn load_with(extra: MapSource) -> bui_env_loader::Loaded<Config> {
    let source = extra.with("APP_PORT", "8080").with("APP_TIMEOUT", "5");
    Loader::new()
        .without_process_environment()
        .source(source)
        .load::<Config>()
        .unwrap()
}

#[test]
fn loaded_handle_exposes_and_releases_the_config() {
    let loaded = load_with(MapSource::new("test"));

    assert_eq!(loaded.value().port, 8080);
    // Deref makes field access ergonomic without `value()`.
    assert_eq!(loaded.timeout, 5);

    let debug = format!("{loaded:?}");
    assert!(debug.contains("Loaded"), "{debug}");

    let config = loaded.into_inner();
    assert_eq!(config.port, 8080);
}

#[test]
fn an_absent_optional_value_falls_back_to_the_typed_default() {
    #[derive(Debug)]
    struct Defaults {
        timeout: u32,
        retries: u32,
    }

    impl EnvConfig for Defaults {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            Ok(Self {
                timeout: context.get_or("APP_TIMEOUT", 30)?,
                retries: context.get_or("APP_RETRIES", 3)?,
            })
        }
    }

    impl Materialize for Defaults {
        type Output = (u32, u32);

        fn materialize(&self, _context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
            Some((self.timeout, self.retries))
        }
    }

    let materialized = Loader::new()
        .without_process_environment()
        .source(MapSource::new("test").with("APP_TIMEOUT", "5"))
        .load::<Defaults>()
        .unwrap()
        .materialize(MaterializationMode::Warn)
        .unwrap();

    assert_eq!(materialized.value, (5, 3));

    let defaults_used: Vec<_> = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::DefaultUsed)
        .collect();
    assert_eq!(defaults_used.len(), 1);
    assert_eq!(defaults_used[0].variable.as_deref(), Some("APP_RETRIES"));
    assert_eq!(defaults_used[0].severity, IssueSeverity::Notice);
}

#[test]
fn invalid_variable_names_are_rejected_with_a_dedicated_kind() {
    #[derive(Debug)]
    struct OneKey;

    impl EnvConfig for OneKey {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            context.get::<u16>("bad-key")?;
            Ok(Self)
        }
    }

    let error = Loader::new()
        .without_process_environment()
        .load::<OneKey>()
        .unwrap_err();

    assert_eq!(error.kind(), ConfigErrorKind::InvalidKey);
    assert_eq!(error.key(), Some("bad-key"));
}

#[test]
fn invalid_values_in_optional_fields_are_still_hard_errors() {
    #[derive(Debug)]
    struct Optional;

    impl EnvConfig for Optional {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            let _: Option<u16> = context.get_optional("APP_PORT")?;
            Ok(Self)
        }
    }

    let error = Loader::new()
        .without_process_environment()
        .source(MapSource::new("test").with("APP_PORT", "not-a-number"))
        .load::<Optional>()
        .unwrap_err();

    assert_eq!(error.kind(), ConfigErrorKind::InvalidValue);
    assert_eq!(error.expected_type(), Some("u16"));
}

#[derive(Debug)]
struct LazyConfig {
    port: Lazy<u16>,
}

impl EnvConfig for LazyConfig {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            port: context.lazy_get("APP_PORT")?,
        })
    }
}

impl Materialize for LazyConfig {
    type Output = u16;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        context.lazy("port", &self.port)
    }
}

#[test]
fn a_failing_lazy_field_becomes_an_error_issue_and_rejects_the_snapshot() {
    let loaded = Loader::new()
        .without_process_environment()
        .source(MapSource::new("test").with("APP_PORT", "not-a-number"))
        .load::<LazyConfig>()
        .unwrap();

    let error: MaterializationError = loaded.materialize(MaterializationMode::Normal).unwrap_err();
    assert!(error.report.has_errors());

    let text = error.to_string();
    assert!(text.contains("1 issue"), "{text}");

    let issue = &error.report.issues()[0];
    assert_eq!(issue.kind, IssueKind::InvalidValue);
    assert_eq!(issue.severity, IssueSeverity::Error);
    assert_eq!(issue.field.as_deref(), Some("port"));
    assert_eq!(issue.variable.as_deref(), Some("APP_PORT"));
}

#[test]
fn undeclared_prefixed_variables_are_flagged_as_unknown() {
    let materialized = Loader::new()
        .without_process_environment()
        .source(
            MapSource::new("test")
                .with("APP_PORT", "8080")
                .with("APP_TIMEOUT", "5")
                .with("APP_ORPHAN", "unused"),
        )
        .load::<Config>()
        .unwrap()
        .materialize(MaterializationMode::Warn)
        .unwrap();

    let unknown: Vec<_> = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::UnknownVariable)
        .collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].variable.as_deref(), Some("APP_ORPHAN"));
    assert_eq!(unknown[0].severity, IssueSeverity::Warning);

    // Warn tolerates the unknown variable: the snapshot still exists.
    assert_eq!(materialized.value.0, 8080);
}

#[test]
fn strict_mode_rejects_an_unknown_variable() {
    let error = Loader::new()
        .without_process_environment()
        .source(
            MapSource::new("test")
                .with("APP_PORT", "8080")
                .with("APP_TIMEOUT", "5")
                .with("APP_ORPHAN", "unused"),
        )
        .load::<Config>()
        .unwrap()
        .materialize(MaterializationMode::Strict)
        .unwrap_err();

    assert!(!error.report.is_clean());
    assert!(
        !error.report.has_errors(),
        "an unknown variable is only a warning"
    );
}

#[test]
fn a_clean_configuration_passes_strict_materialization() {
    let materialized = load_with(MapSource::new("test"))
        .materialize(MaterializationMode::Strict)
        .unwrap();

    assert!(materialized.report.is_clean());
    assert!(!materialized.report.has_errors());
    assert_eq!(materialized.value, (8080, 5));
}

/// Returns `None` without resolving anything, violating the `Materialize`
/// convention that a missing value must be explained by a recorded error.
#[derive(Debug)]
struct BrokenMaterialize;

impl EnvConfig for BrokenMaterialize {
    fn from_env(_context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self)
    }
}

impl Materialize for BrokenMaterialize {
    type Output = ();

    fn materialize(&self, _context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        None
    }
}

#[test]
fn returning_none_without_an_error_produces_an_internal_issue() {
    let error = Loader::new()
        .without_process_environment()
        .load::<BrokenMaterialize>()
        .unwrap()
        .materialize(MaterializationMode::Normal)
        .unwrap_err();

    assert!(error.report.has_errors());
    assert_eq!(error.report.issues()[0].kind, IssueKind::Internal);
}

#[test]
fn issues_are_deduplicated_in_reports() {
    // APP_TIMEOUT is absent, so both the eager load phase and the materialize
    // phase notice the default; the report must list it only once.
    #[derive(Debug)]
    struct Defaults {
        timeout: u32,
    }

    impl EnvConfig for Defaults {
        fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
            Ok(Self {
                timeout: context.get_or("APP_TIMEOUT", 30)?,
            })
        }
    }

    impl Materialize for Defaults {
        type Output = u32;

        fn materialize(&self, _context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
            Some(self.timeout)
        }
    }

    let materialized = Loader::new()
        .without_process_environment()
        .source(MapSource::new("test").with("APP_PORT", "8080"))
        .load::<Defaults>()
        .unwrap()
        .materialize(MaterializationMode::Notify)
        .unwrap();

    let defaults_used = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::DefaultUsed)
        .count();
    assert_eq!(defaults_used, 1);
}
