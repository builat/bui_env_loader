//! The process environment: snapshots, prefixes and opting out.
//!
//! - By default the loader takes a one-time snapshot of the process
//!   environment; later `set_var` calls are invisible to it.
//! - `prefix` scopes unknown-variable auditing to that namespace, so a
//!   program does not flag unrelated variables like PATH or HOME.
//! - `without_process_environment` removes the source entirely, useful for
//!   tests and for configs driven purely by files.
//!
//! Run with: cargo run --example process_environment

use bui_env_loader::{
    ConfigContext, ConfigError, EnvConfig, IssueKind, Loader, MaterializationMode,
};

#[derive(Debug)]
struct Config {
    host: String,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            host: context.get("MYAPP_HOST")?,
        })
    }
}

impl bui_env_loader::Materialize for Config {
    type Output = String;

    fn materialize(
        &self,
        _context: &mut bui_env_loader::MaterializationContext<'_>,
    ) -> Option<Self::Output> {
        Some(self.host.clone())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("MYAPP_HOST", "from-process");
    std::env::set_var("MYAPP_ORPHAN", "nobody-declares-me");

    // Snapshot: the value at load time is frozen into the source.
    let loaded = Loader::new()
        .prefix("MYAPP_") // audit only MYAPP_* variables
        .load::<Config>()?;
    println!("host: {}", loaded.host);

    // Changing the process environment afterwards changes nothing.
    std::env::set_var("MYAPP_HOST", "changed-later");
    let again = Loader::new().prefix("MYAPP_").load::<Config>()?;
    println!(
        "reloaded host: {} (fresh snapshot of a changed env)",
        again.host
    );

    // The orphan MYAPP_ORPHAN variable is flagged because it matches the
    // prefix but no field declares it. Without a prefix, process variables
    // would not be audited at all (PATH, HOME, ... make that too noisy).
    let materialized = loaded.materialize(MaterializationMode::Warn)?;
    for issue in materialized.report.issues() {
        println!("audit: {:?} {:?}", issue.kind, issue.variable);
    }
    let unknown = materialized
        .report
        .issues()
        .iter()
        .filter(|issue| issue.kind == IssueKind::UnknownVariable)
        .count();
    println!("unknown MYAPP_* variables: {unknown}");

    // Opting out: the loader works without any process access, so the same
    // code runs in tests and hermetic environments.
    let hermetic = Loader::new()
        .without_process_environment()
        .set("MYAPP_HOST", "hermetic")
        .load::<Config>()?;
    println!("hermetic host: {}", hermetic.host);

    Ok(())
}
