//! Source precedence and custom sources.
//!
//! Priority order (first match wins):
//!   1. explicit values added with `Loader::set`;
//!   2. custom sources, in registration order;
//!   3. a snapshot of the process environment (unless disabled);
//!   4. dotenv files, in registration order.
//!
//! A key found in several sources keeps the highest-priority value and the
//! remaining matches are reported as duplicate-variable issues.
//!
//! Run with: cargo run --example sources_and_precedence

use bui_env_loader::source::{MapSource, Source};
use bui_env_loader::{
    ConfigContext, ConfigError, EnvConfig, IssueKind, Loader, MaterializationMode, SourceError,
};

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            host: context.get("APP_HOST")?,
            port: context.get("APP_PORT")?,
        })
    }
}

impl bui_env_loader::Materialize for Config {
    type Output = (String, u16);

    fn materialize(
        &self,
        _context: &mut bui_env_loader::MaterializationContext<'_>,
    ) -> Option<Self::Output> {
        Some((self.host.clone(), self.port))
    }
}

/// A custom source can wrap anything: a config server client, a vault, or in
/// this case a hardcoded service catalog.
struct ServiceCatalog;

impl Source for ServiceCatalog {
    fn name(&self) -> &str {
        "service-catalog"
    }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        match key {
            "APP_HOST" => Ok(Some("catalog.internal".to_owned())),
            _ => Ok(None),
        }
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(vec!["APP_HOST".to_owned()]))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = Loader::new()
        .without_process_environment()
        // 1. Highest priority: explicit values.
        .set("APP_HOST", "explicit.example.test")
        .set("APP_PORT", "9000")
        // 2. Custom sources, in order.
        .source(ServiceCatalog)
        .source(MapSource::new("fallback-map").with("APP_HOST", "map.example.test"))
        // 4. Dotenv files come last.
        .add_dotenv_optional("examples/.env.example_base")
        .load::<Config>()?;

    println!(
        "host: {} (the explicit value beat two other sources)",
        loaded.host
    );
    println!("port: {}", loaded.port);

    // A snapshot in Warn mode reports every discrepancy without failing.
    let materialized = loaded.materialize(MaterializationMode::Warn)?;

    for issue in materialized.report.issues() {
        println!(
            "issue {:?} on {:?} from source(s) {:?}",
            issue.kind, issue.variable, issue.source
        );
    }

    // Without the explicit override the catalog would win:
    let catalog_wins = Loader::new()
        .without_process_environment()
        .source(ServiceCatalog)
        .set("APP_PORT", "9000")
        .load::<Config>()?;
    println!("host without override: {}", catalog_wins.host);

    let duplicates = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::DuplicateVariable)
        .count();
    println!("duplicate-variable issues reported: {duplicates}");

    Ok(())
}
