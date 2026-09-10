//! Materilization

use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::deferred::{Computed, Lazy, LazyOptional};
use crate::loader::Loaded;
use crate::reporter::{Event, EventKind, Level};
use crate::resolver::ResolveSession;
use crate::{ConfigError, ConfigErrorKind};

/// Mutually exclusive behavior used when a complete snapshot is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaterializationMode {
    /// Silent for non-fatal discrepancies. Fatal errors are still reported.
    #[default]
    Normal,
    /// Additionally reports missing optional variables at `Info` level.
    Notify,
    /// Reports every discrepancy, while tolerating non-fatal ones.
    Warn,
    /// Reports every discrepancy and rejects a snapshot if any issue exists.
    Strict,
}

/// Whether an issue is informational, suspicious, or prevents a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    Notice,
    Warning,
    Error,
}

/// A stable category for programmatic inspection of materialization reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueKind {
    MissingOptional,
    MissingRequired,
    DefaultUsed,
    UnknownVariable,
    DuplicateVariable,
    InvalidValue,
    ComputationFailed,
    SourceFailed,
    Internal,
}

/// A value-safe discrepancy found while resolving configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializationIssue {
    pub field: Option<Box<str>>,
    pub variable: Option<Box<str>>,
    pub source: Option<Box<str>>,
    pub expected_type: Option<&'static str>,
    pub kind: IssueKind,
    pub severity: IssueSeverity,
    pub message: Box<str>,
}

impl MaterializationIssue {
    pub(crate) fn missing_optional(key: &str) -> Self {
        Self::new(
            None,
            Some(key),
            None,
            None,
            IssueKind::MissingOptional,
            IssueSeverity::Notice,
            "[*] Optional variable is missing",
        )
    }

    pub(crate) fn default_used(key: &str) -> Self {
        Self::new(
            None,
            Some(key),
            None,
            None,
            IssueKind::DefaultUsed,
            IssueSeverity::Notice,
            "[*] Default value was used",
        )
    }

    pub(crate) fn duplicate(key: &str, sources: &[String]) -> Self {
        let source_list = sources.join(", ");
        Self::new(
            None,
            Some(key),
            Some(source_list.as_str()),
            None,
            IssueKind::DuplicateVariable,
            IssueSeverity::Warning,
            "[!] Variable exists in more than one source; the highest-priority source won",
        )
    }

    pub(crate) fn duplicate_in_source(key: &str, source: &str) -> Self {
        Self::new(
            None,
            Some(key),
            Some(source),
            None,
            IssueKind::DuplicateVariable,
            IssueSeverity::Warning,
            "[!] Variable is declared more than once in the same source; the last value won",
        )
    }

    pub(crate) fn unknown(key: &str, source: &str) -> Self {
        Self::new(
            None,
            Some(key),
            Some(source),
            None,
            IssueKind::UnknownVariable,
            IssueSeverity::Warning,
            "[!] Variable is present in a source but is not declared by the configuration",
        )
    }

    pub(crate) fn from_error(field: Option<&str>, error: &ConfigError) -> Self {
        let (kind, severity) = match error.kind() {
            ConfigErrorKind::MissingVariable => (IssueKind::MissingRequired, IssueSeverity::Error),
            ConfigErrorKind::InvalidValue | ConfigErrorKind::InvalidKey => {
                (IssueKind::InvalidValue, IssueSeverity::Error)
            }
            ConfigErrorKind::Computation => (IssueKind::ComputationFailed, IssueSeverity::Error),
            ConfigErrorKind::Source | ConfigErrorKind::Io => {
                (IssueKind::SourceFailed, IssueSeverity::Error)
            }
            ConfigErrorKind::DotenvSyntax | ConfigErrorKind::Internal => {
                (IssueKind::Internal, IssueSeverity::Error)
            }
        };

        Self::new(
            field,
            error.key(),
            error.source_name(),
            error.expected_type(),
            kind,
            severity,
            error.safe_log_message(),
        )
    }

    pub(crate) fn internal(message: &str) -> Self {
        Self::new(
            None,
            None,
            None,
            None,
            IssueKind::Internal,
            IssueSeverity::Error,
            message,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        field: Option<&str>,
        variable: Option<&str>,
        source: Option<&str>,
        expected_type: Option<&'static str>,
        kind: IssueKind,
        severity: IssueSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.map(Into::into),
            variable: variable.map(Into::into),
            source: source.map(Into::into),
            expected_type,
            kind,
            severity,
            message: message.into().into_boxed_str(),
        }
    }
}

/// All non-fatal and fatal findings from one materialization attempt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterializationReport {
    issues: Vec<MaterializationIssue>,
}

impl MaterializationReport {
    pub(crate) fn new(issues: Vec<MaterializationIssue>) -> Self {
        Self {
            issues: deduplicate(issues),
        }
    }

    pub fn issues(&self) -> &[MaterializationIssue] {
        &self.issues
    }

    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
    }
}

/// A successfully created snapshot and its diagnostics.
#[derive(Debug)]
pub struct Materialized<T> {
    pub value: T,
    pub report: MaterializationReport,
}

/// The detailed error returned when materialization cannot be accepted.
#[derive(Debug, Clone)]
pub struct MaterializationError {
    pub report: MaterializationReport,
}

impl Display for MaterializationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "[!!] Configuration materialization failed with {} issue(s)",
            self.report.issues().len()
        )
    }
}

impl Error for MaterializationError {}

/// Describes how an application config becomes a fully owned snapshot.
///
/// The implementation should evaluate every deferred field before deciding
/// whether it can construct `Output`. That convention allows Strict mode to
/// return a useful aggregate report instead of stopping at the first failure.
pub trait Materialize {
    type Output;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output>;
}

/// Field-level helpers backed by one shared [`ResolveSession`].
///
/// Sharing a session is important: if several computed fields request the same
/// variable, they see the same raw value within this one snapshot.
pub struct MaterializationContext<'a> {
    session: &'a mut ResolveSession,
}

impl<'a> MaterializationContext<'a> {
    pub(crate) fn new(session: &'a mut ResolveSession) -> Self {
        Self { session }
    }

    pub fn lazy<T>(&mut self, field: &str, value: &Lazy<T>) -> Option<T>
    where
        T: std::str::FromStr + 'static,
        T::Err: Display,
    {
        match value.resolve_in(self.session) {
            Ok(value) => Some(value),
            Err(error) => {
                self.session
                    .push_issue(MaterializationIssue::from_error(Some(field), &error));
                None
            }
        }
    }

    /// The outer `Option` indicates success or failure. The inner `Option`
    /// represents the actual optional configuration value.
    pub fn lazy_optional<T>(&mut self, field: &str, value: &LazyOptional<T>) -> Option<Option<T>>
    where
        T: std::str::FromStr + 'static,
        T::Err: Display,
    {
        match value.resolve_in(self.session) {
            Ok(value) => Some(value),
            Err(error) => {
                self.session
                    .push_issue(MaterializationIssue::from_error(Some(field), &error));
                None
            }
        }
    }

    pub fn computed<T>(&mut self, field: &str, value: &Computed<T>) -> Option<T> {
        match value.resolve_in(self.session) {
            Ok(value) => Some(value),
            Err(error) => {
                self.session
                    .push_issue(MaterializationIssue::from_error(Some(field), &error));
                None
            }
        }
    }
}

impl<C> Loaded<C>
where
    C: Materialize,
{
    /// Evaluates all deferred fields once and creates a typed snapshot.
    pub fn materialize(
        &self,
        mode: MaterializationMode,
    ) -> Result<Materialized<C::Output>, MaterializationError> {
        self.runtime.emit(Event {
            level: Level::Info,
            kind: EventKind::MaterializationStarted,
            field: None,
            key: None,
            source: None,
            message: "[+] Configuration materialization started",
        });

        let mut session = ResolveSession::new(self.runtime.clone());
        let value = {
            let mut context = MaterializationContext::new(&mut session);
            self.config.materialize(&mut context)
        };

        let mut issues = self.load_issues.clone();
        issues.extend(session.finish());
        issues.extend(self.runtime.unknown_variable_issues());

        if value.is_none()
            && !issues
                .iter()
                .any(|issue| issue.severity == IssueSeverity::Error)
        {
            issues.push(MaterializationIssue::internal(
                "[!!] The Materialize implementation returned no value without recording an error",
            ));
        }

        let report = MaterializationReport::new(issues);
        log_report(&self.runtime, mode, &report);

        let rejected =
            report.has_errors() || (mode == MaterializationMode::Strict && !report.is_clean());

        if rejected {
            self.runtime.emit(Event {
                level: Level::Error,
                kind: EventKind::MaterializationFinished,
                field: None,
                key: None,
                source: None,
                message: "[!!] Configuration materialization failed",
            });
            return Err(MaterializationError { report });
        }

        self.runtime.emit(Event {
            level: Level::Info,
            kind: EventKind::MaterializationFinished,
            field: None,
            key: None,
            source: None,
            message: "[+] Configuration materialization finished",
        });

        Ok(Materialized {
            value: value.expect("a missing value was converted into an error above"),
            report,
        })
    }
}

fn log_report(
    runtime: &crate::resolver::Runtime,
    mode: MaterializationMode,
    report: &MaterializationReport,
) {
    for issue in report.issues() {
        let should_log = match mode {
            MaterializationMode::Normal => issue.severity == IssueSeverity::Error,
            MaterializationMode::Notify => {
                issue.severity == IssueSeverity::Error || issue.kind == IssueKind::MissingOptional
            }
            MaterializationMode::Warn | MaterializationMode::Strict => true,
        };

        if !should_log {
            continue;
        }

        let level = match (mode, issue.severity) {
            (_, IssueSeverity::Error) => Level::Error,
            (MaterializationMode::Notify, _) => Level::Info,
            (_, IssueSeverity::Warning) => Level::Warn,
            (_, IssueSeverity::Notice) => Level::Info,
        };

        runtime.emit(Event {
            level,
            kind: EventKind::Issue,
            field: issue.field.as_deref(),
            key: issue.variable.as_deref(),
            source: issue.source.as_deref(),
            message: &issue.message,
        });
    }
}

fn deduplicate(issues: Vec<MaterializationIssue>) -> Vec<MaterializationIssue> {
    let mut unique = Vec::new();
    for issue in issues {
        if !unique.contains(&issue) {
            unique.push(issue);
        }
    }
    unique
}
