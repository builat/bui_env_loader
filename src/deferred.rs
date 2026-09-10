//! Deferred source-backed and function-backed fields.

use std::fmt::{self, Debug, Display, Formatter};
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use crate::reporter::{Event, EventKind, Level};
use crate::resolver::{ResolveSession, Runtime};
use crate::ConfigError;

/// A required variable that is looked up and parsed on every [`Lazy::get`].
pub struct Lazy<T> {
    key: Box<str>,
    runtime: Arc<Runtime>,
    // `fn() -> T` records the generic result type without pretending that the
    // wrapper owns a T before resolution occurs.
    marker: PhantomData<fn() -> T>,
}

impl<T> Lazy<T> {
    pub(crate) fn new(key: &str, runtime: Arc<Runtime>) -> Self {
        Self {
            key: key.into(),
            runtime,
            marker: PhantomData,
        }
    }
}

impl<T> Lazy<T>
where
    T: FromStr + 'static,
    T::Err: Display,
{
    pub fn get(&self) -> Result<T, ConfigError> {
        let mut session = ResolveSession::new(self.runtime.clone());
        self.resolve_in(&mut session)
    }

    pub(crate) fn resolve_in(&self, session: &mut ResolveSession) -> Result<T, ConfigError> {
        ensure_same_runtime(session, &self.runtime)?;
        session.get(&self.key)
    }
}

impl<T> Debug for Lazy<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Lazy")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// An optional variable looked up and parsed on every call.
pub struct LazyOptional<T> {
    key: Box<str>,
    runtime: Arc<Runtime>,
    marker: PhantomData<fn() -> T>,
}

impl<T> LazyOptional<T> {
    pub(crate) fn new(key: &str, runtime: Arc<Runtime>) -> Self {
        Self {
            key: key.into(),
            runtime,
            marker: PhantomData,
        }
    }
}

impl<T> LazyOptional<T>
where
    T: FromStr + 'static,
    T::Err: Display,
{
    pub fn lazy_get_optional(&self) -> Result<Option<T>, ConfigError> {
        let mut session = ResolveSession::new(self.runtime.clone());
        self.resolve_in(&mut session)
    }

    pub(crate) fn resolve_in(
        &self,
        session: &mut ResolveSession,
    ) -> Result<Option<T>, ConfigError> {
        ensure_same_runtime(session, &self.runtime)?;
        session.get_optional(&self.key)
    }
}

impl<T> Debug for LazyOptional<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LazyOptional")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

type ComputeFunction<T> =
    dyn Fn(&mut ResolveSession) -> Result<T, ConfigError> + Send + Sync + 'static;

/// A function-backed field whose closure runs on every [`Computed::get`].
pub struct Computed<T> {
    name: Box<str>,
    runtime: Arc<Runtime>,
    function: Box<ComputeFunction<T>>,
}

impl<T> Computed<T> {
    pub(crate) fn new<F>(name: &str, runtime: Arc<Runtime>, function: F) -> Self
    where
        F: Fn(&mut ResolveSession) -> Result<T, ConfigError> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            runtime,
            function: Box::new(function),
        }
    }

    pub fn get(&self) -> Result<T, ConfigError> {
        let mut session = ResolveSession::new(self.runtime.clone());
        self.resolve_in(&mut session)
    }

    pub(crate) fn resolve_in(&self, session: &mut ResolveSession) -> Result<T, ConfigError> {
        ensure_same_runtime(session, &self.runtime)?;

        self.runtime.emit(Event {
            level: Level::Trace,
            kind: EventKind::ComputationStarted,
            field: Some(&self.name),
            key: None,
            source: None,
            message: "computed field evaluation started",
        });

        let result = (self.function)(session);
        match &result {
            Ok(_) => self.runtime.emit(Event {
                level: Level::Debug,
                kind: EventKind::ComputationFinished,
                field: Some(&self.name),
                key: None,
                source: None,
                message: "computed field evaluation finished",
            }),
            Err(error) => self.runtime.emit(Event {
                level: Level::Error,
                kind: EventKind::ComputationFinished,
                field: Some(&self.name),
                key: error.key(),
                source: error.source_name(),
                message: error.safe_log_message(),
            }),
        }

        result
    }
}

impl<T> Debug for Computed<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Computed")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

fn ensure_same_runtime(
    session: &ResolveSession,
    runtime: &Arc<Runtime>,
) -> Result<(), ConfigError> {
    if session.belongs_to(runtime) {
        Ok(())
    } else {
        Err(ConfigError::internal(
            "a deferred field was materialized by a different Loader instance",
        ))
    }
}
