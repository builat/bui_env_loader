//! Dotenv files: required vs. optional registration and layering.
//!
//! - `add_dotenv` makes the file mandatory: a missing file fails `load` with
//!   an `Io` error, and malformed syntax fails with `DotenvSyntax`.
//! - `add_dotenv_optional` skips files that do not exist, which is useful for
//!   `.env.local`-style developer overrides.
//! - Files are consulted in registration order; the first one containing a
//!   key wins.
//!
//! Run with: cargo run --example dotenv_layering

use bui_env_loader::{
    ConfigContext, ConfigError, ConfigErrorKind, EnvConfig, Loader, MaterializationMode,
};

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    log_level: String,
    token: Option<String>,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            host: context.get("APP_HOST")?,
            port: context.get("APP_PORT")?,
            log_level: context.get("APP_LOG_LEVEL")?,
            token: context.get_optional("APP_TOKEN")?,
        })
    }
}

impl bui_env_loader::Materialize for Config {
    type Output = (String, u16, String, Option<String>);

    fn materialize(
        &self,
        _context: &mut bui_env_loader::MaterializationContext<'_>,
    ) -> Option<Self::Output> {
        Some((
            self.host.clone(),
            self.port,
            self.log_level.clone(),
            self.token.clone(),
        ))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = Loader::new()
        .without_process_environment()
        // Base layer exists: its values are loaded.
        .add_dotenv("examples/.env.example_base")
        // Local overrides exist in this checkout: extra keys are picked up.
        .add_dotenv_optional("examples/.env.example_local")
        // A typical developer machine may not have this file; it is skipped.
        .add_dotenv_optional("examples/.env.example_secrets")
        .load::<Config>()?;

    println!("host:      {} (from the base layer)", loaded.host);
    println!("port:      {}", loaded.port);
    println!("log_level: {}", loaded.log_level);
    println!("token:     {:?} (from the local layer)", loaded.token);

    // A missing REQUIRED file is a hard error.
    let error = Loader::new()
        .without_process_environment()
        .add_dotenv("examples/.env.example_missing")
        .load::<Config>()
        .unwrap_err();
    println!("missing required file -> {:?}: {}", error.kind(), error);

    // So is malformed dotenv syntax. See the fixture for the offending line.
    let error = Loader::new()
        .without_process_environment()
        .add_dotenv("examples/.env.example_broken")
        .load::<Config>()
        .unwrap_err();
    assert_eq!(error.kind(), ConfigErrorKind::DotenvSyntax);
    println!("malformed file         -> {}", error);

    // Dotenv duplicate keys are reported but tolerated in Normal mode.
    let duplicated = Loader::new()
        .without_process_environment()
        .add_dotenv("examples/.env.example_duplicate")
        .set("APP_HOST", "explicit-override")
        .set("APP_PORT", "1")
        .set("APP_LOG_LEVEL", "info")
        .load::<Config>()?;
    let report = duplicated.materialize(MaterializationMode::Warn).unwrap();
    println!(
        "duplicate key in file  -> {:?}",
        report
            .report
            .issues()
            .iter()
            .map(|issue| (issue.kind, issue.variable.as_deref()))
            .collect::<Vec<_>>()
    );

    Ok(())
}
