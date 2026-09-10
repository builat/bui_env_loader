//! Interface for the condig typed constructors and macro generation.

use std::fmt::Display;
use std::str::FromStr;

use crate::deferred::{Computed, Lazy, LazyOptional};
use crate::materialize::MaterializationIssue;
use crate::resolver::{ResolveSession, Runtime};
use crate::ConfigError;
use std::sync::Arc;

/// Implemented by an application's strongly typed configuration structure.
pub trait EnvConfig: Sized {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError>;
}

pub struct ConfigContext {
    session: ResolveSession,
}

impl ConfigContext {
    pub(crate) fn new(runtime: Arc<Runtime>) -> Self {
        Self {
            session: ResolveSession::new(runtime),
        }
    }

    /// Reads and parses a required variable immediately.
    pub fn get<T>(&mut self, key: &str) -> Result<T, ConfigError>
    where
        T: FromStr,
        T::Err: Display,
    {
        self.session.get(key)
    }

    /// Reads an optional variable immediately.
    pub fn get_optional<T>(&mut self, key: &str) -> Result<Option<T>, ConfigError>
    where
        T: FromStr,
        T::Err: Display,
    {
        self.session.get_optional(key)
    }

    /// Reads immediately or uses a typed default when the key is absent.
    pub fn get_or<T>(&mut self, key: &str, default: T) -> Result<T, ConfigError>
    where
        T: FromStr,
        T::Err: Display,
    {
        self.session.get_or(key, default)
    }

    /// Creates a required field whose lookup is deferred until `.get()`.
    pub fn lazy_get<T>(&mut self, key: &str) -> Result<Lazy<T>, ConfigError>
    where
        T: FromStr + 'static,
        T::Err: Display,
    {
        // Validate and register now, but intentionally do not read the source.
        self.session.register_deferred_key(key)?;
        Ok(Lazy::new(key, self.session.runtime()))
    }

    /// Creates an optional field whose lookup is deferred until `.get()`.
    pub fn lazy_get_optional<T>(&mut self, key: &str) -> Result<LazyOptional<T>, ConfigError>
    where
        T: FromStr + 'static,
        T::Err: Display,
    {
        self.session.register_deferred_key(key)?;
        Ok(LazyOptional::new(key, self.session.runtime()))
    }

    /// Creates a closure-backed field. The closure is not run here.
    ///
    /// Every invocation receives a fresh session during `Computed::get`, or the
    /// shared snapshot session during whole-config materialization.
    ///
    /// Could be used to compute a value based on other config values, or to perform
    /// more complex logic that cannot be expressed as a simple type conversion.
    ///
    /// Also good for values that are expensive to compute and should only be computed when needed.
    ///
    /// OR
    ///
    /// Could be used when values are not known at compile time and must be computed at runtime,
    /// such as when values depend on external services or APIs.
    ///
    /// Async not supported yet, but could be added in the future.
    pub fn get_computed<T, F>(&mut self, name: &str, function: F) -> Computed<T>
    where
        F: Fn(&mut ResolveSession) -> Result<T, ConfigError> + Send + Sync + 'static,
    {
        Computed::new(name, self.session.runtime(), function)
    }

    pub(crate) fn finish(self) -> Vec<MaterializationIssue> {
        self.session.finish()
    }
}
