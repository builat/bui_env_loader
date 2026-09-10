//! `typed_env` is a small, dependency-free configuration library.
//!
//! The crate deliberately uses explicit traits instead of reflection or a
//! procedural macro. This keeps the first version approachable for learning:
//! every conversion, ownership boundary and error path is visible in ordinary
//! Rust code.
//!
//! The usual flow is:
//!
//! 1. Implement [`EnvConfig`] for an application-specific structure.
//! 2. Construct it with [`Loader::load`].
//! 3. Read deferred values through [`Lazy::get`] or [`Computed::get`].
//! 4. Optionally turn the whole configuration into an immutable snapshot with
//!    [`Loaded::materialize`].

mod context;
mod deferred;
mod error;
mod loader;
mod macros;
mod materialize;
mod reporter;
mod resolver;
pub mod source;

pub use context::{ConfigContext, EnvConfig};
pub use deferred::{Computed, Lazy, LazyOptional};
pub use error::{ConfigError, ConfigErrorKind, SourceError};
pub use loader::{Loaded, Loader};
pub use materialize::{
    IssueKind, IssueSeverity, MaterializationContext, MaterializationError, MaterializationIssue,
    MaterializationMode, MaterializationReport, Materialize, Materialized,
};
pub use reporter::{Event, EventKind, Level, Reporter};
pub use resolver::ResolveSession;
