//! Error handling: matching `ConfigErrorKind` and presenting safe messages.
//!
//! Every failure carries a machine-readable kind plus key/source/type
//! context. `Display` output is safe for end users because raw values are
//! never stored in the error.
//!
//! Run with: cargo run --example error_handling

use bui_env_loader::{
    Computed, ConfigContext, ConfigError, ConfigErrorKind, EnvConfig, Loader, ResolveSession,
};

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    version: Computed<String>,
}

impl EnvConfig for Config {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            host: context.get("APP_HOST")?,
            port: context.get("APP_PORT")?,
            version: context.get_computed("version", |session: &mut ResolveSession| {
                let host = session.get::<String>("APP_HOST")?;
                if host == "forbidden.example.test" {
                    // Computed fields report failures with their own kind.
                    return Err(ConfigError::computation(
                        "version",
                        "host is not allowed to serve a version endpoint",
                    ));
                }
                Ok(format!("v1 on {host}"))
            }),
        })
    }
}

fn describe(result: Result<bui_env_loader::Loaded<Config>, ConfigError>) {
    match result {
        Ok(_) => println!("ok (unexpected)"),
        Err(error) => {
            // Match the kind first; the context accessors fill in details.
            match error.kind() {
                ConfigErrorKind::MissingVariable => {
                    println!("missing:   key={:?} :: {}", error.key(), error);
                }
                ConfigErrorKind::InvalidValue => {
                    println!(
                        "invalid:   key={:?} expected={:?} :: {}",
                        error.key(),
                        error.expected_type(),
                        error
                    );
                }
                ConfigErrorKind::InvalidKey => {
                    println!("bad key:   {}", error);
                }
                ConfigErrorKind::Computation => {
                    println!("computed:  {}", error);
                }
                ConfigErrorKind::DotenvSyntax | ConfigErrorKind::Io | ConfigErrorKind::Source => {
                    println!("source:    in={:?} :: {}", error.source_name(), error);
                }
                other => println!("other:     {other:?} :: {error}"),
            }
        }
    }
}

fn loader() -> Loader {
    Loader::new().without_process_environment()
}

fn main() {
    // A required variable is absent.
    describe(loader().load::<Config>());

    // A value cannot be parsed into the field's type. Note that the raw value
    // (which could be a secret) never appears in the message.
    describe(
        loader()
            .set("APP_HOST", "example.test")
            .set("APP_PORT", "eighty")
            .load::<Config>(),
    );

    // Computed fields are deferred: their failures surface on `.get()`, not
    // during load. The load succeeds even though the closure will reject.
    let loaded = loader()
        .set("APP_HOST", "forbidden.example.test")
        .set("APP_PORT", "8080")
        .load::<Config>()
        .expect("computed closures are not run during load");

    let error = loaded.version.get().unwrap_err();
    println!("computed:  key={:?} :: {}", error.key(), error);

    // Successful load: computed errors can also surface later, on `.get()`.
    let loaded = loader()
        .set("APP_HOST", "example.test")
        .set("APP_PORT", "8080")
        .load::<Config>()
        .expect("all values are valid");
    println!(
        "computed version: {} for host {}:{}",
        loaded.version.get().unwrap(),
        loaded.host,
        loaded.port
    );

    // The same ConfigError type is a std::error::Error, so it composes with
    // Box<dyn Error> and the ? operator used in other examples.
}
