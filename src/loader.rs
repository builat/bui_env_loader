//! Loader construction, source precedence and the `Loaded<T>` handle.

use std::collections::HashMap;

use std::fmt::Debug;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use crate::context::{ConfigContext, EnvConfig};
use crate::materialize::{MaterializationIssue, MaterializationReport};
use crate::reporter::{Event, EventKind, Level, NoopReporter, Reporter};
use crate::resolver::Runtime;
use crate::source::{DotenvFile, MapSource, ParsedDotenv, ProcessEnvSource, Source};
use crate::ConfigError;

/// Builds a configuration from ordered string sources.
///
/// Source precedence in version 0.1 is:
///
/// 1. values added with [`Loader::set`];
/// 2. custom sources, in registration order;
/// 3. a snapshot of the process environment;
/// 4. dotenv files, in registration order.
///
/// The first source containing a key wins. Other matches are retained as a
/// duplicate-variable issue for Warn and Strict materialization.
pub struct Loader {
    explicit_values: HashMap<String, String>,
    custom_sources: Vec<Arc<dyn Source>>,
    dotenv_files: Vec<DotenvFile>,
    include_process_environment: bool,
    prefix: Option<String>,
    reporter: Arc<dyn Reporter>,
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

impl Loader {
    pub fn new() -> Self {
        Self {
            explicit_values: HashMap::new(),
            custom_sources: Vec::new(),
            dotenv_files: Vec::new(),
            include_process_environment: true,
            prefix: None,
            reporter: Arc::new(NoopReporter),
        }
    }

    /// Adds a highest-priority explicit value.
    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.explicit_values.insert(key.into(), value.into());
        self
    }

    /// Adds an application-defined source above process and dotenv sources.
    pub fn source<S>(mut self, source: S) -> Self
    where
        S: Source + 'static,
    {
        self.custom_sources.push(Arc::new(source));
        self
    }

    /// Registers a required dotenv file. It is opened during [`Loader::load`].
    pub fn add_dotenv(mut self, path: impl Into<PathBuf>) -> Self {
        self.dotenv_files.push(DotenvFile {
            path: path.into(),
            required: true,
        });
        self
    }

    /// Registers a dotenv file that is ignored when it does not exist.
    pub fn add_dotenv_optional(mut self, path: impl Into<PathBuf>) -> Self {
        self.dotenv_files.push(DotenvFile {
            path: path.into(),
            required: false,
        });
        self
    }

    /// Disables the default process-environment snapshot.
    /// by default, the process environment is included as a source.
    /// default value == true
    pub fn without_process_environment(mut self) -> Self {
        self.include_process_environment = false;
        self
    }

    /// Limits unknown-process-variable checks to keys with this prefix.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Installs a framework-agnostic event reporter.
    pub fn reporter<R>(mut self, reporter: R) -> Self
    where
        R: Reporter,
    {
        self.reporter = Arc::new(reporter);
        self
    }

    /// Constructs a strongly typed application configuration.
    ///
    /// Eager fields currently use normal `Result` propagation and therefore
    /// stop at the first fatal error. Deferred fields are aggregated later by
    /// `Loaded::materialize`. A declarative schema can remove this limitation
    /// in a future version without changing the source/resolver design.
    pub fn load<C>(self) -> Result<Loaded<C>, ConfigError>
    where
        C: EnvConfig,
    {
        self.reporter.report(&Event {
            level: Level::Info,
            kind: EventKind::LoadStarted,
            field: None,
            key: None,
            source: None,
            message: "configuration loading started",
        });

        let mut sources: Vec<Arc<dyn Source>> = Vec::new();
        let mut initial_issues = Vec::new();

        if !self.explicit_values.is_empty() {
            let mut explicit = MapSource::new("explicit");
            for (key, value) in self.explicit_values {
                explicit.insert(key, value);
            }
            sources.push(Arc::new(explicit));
        }

        sources.extend(self.custom_sources);

        if self.include_process_environment {
            sources.push(Arc::new(ProcessEnvSource::snapshot()));
        }

        for dotenv_file in self.dotenv_files {
            if !dotenv_file.required {
                match fs::metadata(&dotenv_file.path) {
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        let config_error =
                            ConfigError::io(dotenv_file.path.display().to_string(), error);
                        self.reporter.report(&Event {
                            level: Level::Error,
                            kind: EventKind::Issue,
                            field: None,
                            key: None,
                            source: config_error.source_name(),
                            message: config_error.safe_log_message(),
                        });
                        return Err(config_error);
                    }
                }
            }

            let parsed = match ParsedDotenv::from_path(&dotenv_file.path) {
                Ok(parsed) => parsed,
                Err(error) => {
                    self.reporter.report(&Event {
                        level: Level::Error,
                        kind: EventKind::Issue,
                        field: None,
                        key: error.key(),
                        source: error.source_name(),
                        message: error.safe_log_message(),
                    });
                    return Err(error);
                }
            };

            let source_name = parsed.source.name().to_owned();
            for key in &parsed.duplicate_keys {
                initial_issues.push(MaterializationIssue::duplicate_in_source(key, &source_name));
            }

            self.reporter.report(&Event {
                level: Level::Info,
                kind: EventKind::DotenvLoaded,
                field: None,
                key: None,
                source: Some(&source_name),
                message: "dotenv file loaded",
            });
            sources.push(Arc::new(parsed.source));
        }

        let runtime = Arc::new(Runtime::new(sources, self.reporter, self.prefix));
        let mut context = ConfigContext::new(runtime.clone());

        let value = match C::from_env(&mut context) {
            Ok(value) => value,
            Err(error) => {
                runtime.emit_error(&error);
                return Err(error);
            }
        };

        initial_issues.extend(context.finish());

        runtime.emit(Event {
            level: Level::Info,
            kind: EventKind::LoadFinished,
            field: None,
            key: None,
            source: None,
            message: "configuration loading finished",
        });

        Ok(Loaded {
            config: value,
            runtime,
            load_issues: MaterializationReport::new(initial_issues).issues().to_vec(),
        })
    }
}

/// A loaded config plus the runtime needed by deferred fields and snapshots.
pub struct Loaded<C> {
    pub(crate) config: C,
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) load_issues: Vec<MaterializationIssue>,
}

impl<C: Debug> Debug for Loaded<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Loaded")
            .field("config", &self.config)
            .field("load_issues", &self.load_issues)
            .finish()
    }
}

impl<C> Loaded<C> {
    pub fn value(&self) -> &C {
        &self.config
    }

    pub fn into_inner(self) -> C {
        self.config
    }
}

/// Deref keeps ordinary config access ergonomic: `loaded.host` works just like
/// `loaded.value().host` while the wrapper retains its runtime metadata.
impl<C> Deref for Loaded<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}
