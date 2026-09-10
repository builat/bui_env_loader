//! Typed lookup and per-operation caching.

use std::any::type_name;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::materialize::MaterializationIssue;
use crate::reporter::{Event, EventKind, Level, Reporter};
use crate::source::{is_valid_key, Source};
use crate::ConfigError;

#[derive(Debug, Clone)]
struct ResolvedRaw {
    value: String,
    source: String,
}

/// Shared immutable runtime state retained by `Loaded`, `Lazy` and `Computed`.
pub(crate) struct Runtime {
    sources: Vec<Arc<dyn Source>>,
    reporter: Arc<dyn Reporter>,
    prefix: Option<Box<str>>,
    declared_keys: Mutex<HashSet<String>>,
}

impl Runtime {
    pub(crate) fn new(
        sources: Vec<Arc<dyn Source>>,
        reporter: Arc<dyn Reporter>,
        prefix: Option<String>,
    ) -> Self {
        Self {
            sources,
            reporter,
            prefix: prefix.map(String::into_boxed_str),
            declared_keys: Mutex::new(HashSet::new()),
        }
    }

    pub(crate) fn emit(&self, event: Event<'_>) {
        self.reporter.report(&event);
    }

    pub(crate) fn emit_error(&self, error: &ConfigError) {
        self.emit(Event {
            level: Level::Error,
            kind: EventKind::Issue,
            field: None,
            key: error.key(),
            source: error.source_name(),
            message: error.safe_log_message(),
        });
    }

    fn register_key(&self, key: &str) {
        recover_lock(&self.declared_keys).insert(key.to_owned());
    }

    pub(crate) fn unknown_variable_issues(&self) -> Vec<MaterializationIssue> {
        let declared = recover_lock(&self.declared_keys).clone();
        let mut issues = Vec::new();

        for source in &self.sources {
            // Auditing every process variable without a prefix would flag
            // unrelated keys such as PATH and HOME. File and map sources are
            // still audited in that case.
            if self.prefix.is_none() && source.name() == "process" {
                continue;
            }

            let keys = match source.keys() {
                Ok(Some(keys)) => keys,
                Ok(None) => continue,
                Err(error) => {
                    let error = ConfigError::source(source.name(), None, error);
                    issues.push(MaterializationIssue::from_error(None, &error));
                    continue;
                }
            };

            for key in keys {
                if let Some(prefix) = self.prefix.as_deref() {
                    if !key.starts_with(prefix) {
                        continue;
                    }
                }

                if !declared.contains(&key) {
                    issues.push(MaterializationIssue::unknown(&key, source.name()));
                }
            }
        }

        issues
    }
}

/// A single, consistent lookup session.
///
/// Raw strings are cached for the lifetime of this object. Parsing is still
/// type-specific and happens on every typed call. A normal lazy access creates
/// a fresh session; whole-structure materialization shares one session across
/// all fields.
pub struct ResolveSession {
    runtime: Arc<Runtime>,
    raw_cache: HashMap<String, Result<Option<ResolvedRaw>, ConfigError>>,
    issues: Vec<MaterializationIssue>,
}

impl ResolveSession {
    pub(crate) fn new(runtime: Arc<Runtime>) -> Self {
        Self {
            runtime,
            raw_cache: HashMap::new(),
            issues: Vec::new(),
        }
    }

    /// Resolves a required variable and parses it as `T`.
    pub fn get<T>(&mut self, key: &str) -> Result<T, ConfigError>
    where
        T: FromStr,
        T::Err: Display,
    {
        self.validate_and_register(key)?;

        let Some(raw) = self.read_raw(key)? else {
            let error = ConfigError::missing(key);
            self.runtime.emit_error(&error);
            return Err(error);
        };

        raw.value.parse::<T>().map_err(|reason| {
            let error = ConfigError::invalid_value(key, type_name::<T>(), reason);
            self.runtime.emit_error(&error);
            error
        })
    }

    /// Resolves an optional variable.
    ///
    /// `None` only means that the key is absent. An existing malformed value is
    /// still a hard error, preserving strict typing.
    pub fn get_optional<T>(&mut self, key: &str) -> Result<Option<T>, ConfigError>
    where
        T: FromStr,
        T::Err: Display,
    {
        self.validate_and_register(key)?;

        let Some(raw) = self.read_raw(key)? else {
            self.push_issue(MaterializationIssue::missing_optional(key));
            return Ok(None);
        };

        raw.value.parse::<T>().map(Some).map_err(|reason| {
            let error = ConfigError::invalid_value(key, type_name::<T>(), reason);
            self.runtime.emit_error(&error);
            error
        })
    }

    /// Resolves a variable or records that a default value was used.
    pub fn get_or<T>(&mut self, key: &str, default: T) -> Result<T, ConfigError>
    where
        T: FromStr,
        T::Err: Display,
    {
        self.validate_and_register(key)?;

        let Some(raw) = self.read_raw(key)? else {
            self.push_issue(MaterializationIssue::default_used(key));
            return Ok(default);
        };

        raw.value.parse::<T>().map_err(|reason| {
            let error = ConfigError::invalid_value(key, type_name::<T>(), reason);
            self.runtime.emit_error(&error);
            error
        })
    }

    pub(crate) fn runtime(&self) -> Arc<Runtime> {
        self.runtime.clone()
    }

    pub(crate) fn register_deferred_key(&self, key: &str) -> Result<(), ConfigError> {
        self.validate_and_register(key)
    }

    pub(crate) fn belongs_to(&self, runtime: &Arc<Runtime>) -> bool {
        Arc::ptr_eq(&self.runtime, runtime)
    }

    pub(crate) fn push_issue(&mut self, issue: MaterializationIssue) {
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }

    pub(crate) fn finish(self) -> Vec<MaterializationIssue> {
        self.issues
    }

    fn validate_and_register(&self, key: &str) -> Result<(), ConfigError> {
        if !is_valid_key(key) {
            let error = ConfigError::invalid_key(key);
            self.runtime.emit_error(&error);
            return Err(error);
        }

        self.runtime.register_key(key);
        Ok(())
    }

    fn read_raw(&mut self, key: &str) -> Result<Option<ResolvedRaw>, ConfigError> {
        if let Some(cached) = self.raw_cache.get(key) {
            return cached.clone();
        }

        self.runtime.emit(Event {
            level: Level::Trace,
            kind: EventKind::VariableLookup,
            field: None,
            key: Some(key),
            source: None,
            message: "[+] Looking up configuration variable",
        });

        let result = self.lookup_sources(key);
        self.raw_cache.insert(key.to_owned(), result.clone());
        result
    }

    fn lookup_sources(&mut self, key: &str) -> Result<Option<ResolvedRaw>, ConfigError> {
        let mut matches = Vec::new();

        for source in &self.runtime.sources {
            match source.get(key) {
                Ok(Some(value)) => matches.push(ResolvedRaw {
                    value,
                    source: source.name().to_owned(),
                }),
                Ok(None) => {}
                Err(error) => {
                    let error = ConfigError::source(source.name(), Some(key), error);
                    self.runtime.emit_error(&error);
                    return Err(error);
                }
            }
        }

        if matches.len() > 1 {
            let sources: Vec<String> = matches.iter().map(|value| value.source.clone()).collect();
            self.push_issue(MaterializationIssue::duplicate(key, &sources));
        }

        let selected = matches.into_iter().next();
        if let Some(value) = &selected {
            self.runtime.emit(Event {
                level: Level::Debug,
                kind: EventKind::VariableResolved,
                field: None,
                key: Some(key),
                source: Some(&value.source),
                message: "[+] Configuration variable was resolved",
            });
        }

        Ok(selected)
    }
}

fn recover_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Poisoning means another thread panicked while holding the lock. The data
    // is still structurally valid for HashSet access, so this small diagnostic
    // registry can safely recover instead of crashing the application again.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
