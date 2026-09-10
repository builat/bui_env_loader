//! Error types used by the public API and custom configuration sources.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// A machine-readable category for a configuration error.
///
/// Callers should prefer matching this enum over parsing the human-readable
/// text produced by [`Display`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigErrorKind {
    MissingVariable,
    InvalidValue,
    InvalidKey,
    DotenvSyntax,
    Io,
    Source,
    Computation,
    Internal,
}

/// A configuration failure with enough context for diagnostics.
///
/// The raw variable value is intentionally not stored. Configuration values
/// frequently contain passwords and tokens, so retaining or logging them by
/// default would be unsafe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    kind: ConfigErrorKind,
    key: Option<Box<str>>,
    source: Option<Box<str>>,
    expected_type: Option<&'static str>,
    message: Box<str>,
}

impl ConfigError {
    pub(crate) fn missing(key: &str) -> Self {
        Self::new(
            ConfigErrorKind::MissingVariable,
            Some(key),
            None,
            None,
            "[!!] Required variable is missing",
        )
    }

    pub(crate) fn invalid_value(
        key: &str,
        expected_type: &'static str,
        reason: impl Display,
    ) -> Self {
        Self::new(
            ConfigErrorKind::InvalidValue,
            Some(key),
            None,
            Some(expected_type),
            format!("[!!] Value cannot be parsed: {reason}"),
        )
    }

    pub(crate) fn invalid_key(key: &str) -> Self {
        Self::new(
            ConfigErrorKind::InvalidKey,
            Some(key),
            None,
            None,
            "[!!] Variable name is not a valid environment key",
        )
    }

    pub(crate) fn dotenv(source: impl Into<String>, message: impl Into<String>) -> Self {
        let source = source.into();
        Self::new(
            ConfigErrorKind::DotenvSyntax,
            None,
            Some(source.as_str()),
            None,
            message,
        )
    }

    pub(crate) fn io(source: impl Into<String>, error: impl Display) -> Self {
        let source = source.into();
        Self::new(
            ConfigErrorKind::Io,
            None,
            Some(source.as_str()),
            None,
            error.to_string(),
        )
    }

    pub(crate) fn source(source: &str, key: Option<&str>, error: impl Display) -> Self {
        Self::new(
            ConfigErrorKind::Source,
            key,
            Some(source),
            None,
            error.to_string(),
        )
    }

    /// Creates an error returned by a user-provided computed field.
    pub fn computation(name: &str, error: impl Display) -> Self {
        Self::new(
            ConfigErrorKind::Computation,
            Some(name),
            None,
            None,
            error.to_string(),
        )
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(ConfigErrorKind::Internal, None, None, None, message)
    }

    fn new(
        kind: ConfigErrorKind,
        key: Option<&str>,
        source: Option<&str>,
        expected_type: Option<&'static str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            key: key.map(Into::into),
            source: source.map(Into::into),
            expected_type,
            message: message.into().into_boxed_str(),
        }
    }

    pub fn kind(&self) -> ConfigErrorKind {
        self.kind
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn expected_type(&self) -> Option<&'static str> {
        self.expected_type
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns a value-safe message suitable for automatic logging.
    ///
    /// In particular, this method never includes the raw environment value or
    /// a custom parser's error text, either of which might contain a secret.
    pub(crate) fn safe_log_message(&self) -> &'static str {
        match self.kind {
            ConfigErrorKind::MissingVariable => "[!!] Required variable is missing",
            ConfigErrorKind::InvalidValue => "[!!] Variable has an invalid value",
            ConfigErrorKind::InvalidKey => "[!!] Variable name is invalid",
            ConfigErrorKind::DotenvSyntax => "[!!] Dotenv file contains invalid syntax",
            ConfigErrorKind::Io => "[!!] Configuration file cannot be read",
            ConfigErrorKind::Source => "[!!] Configuration source failed",
            ConfigErrorKind::Computation => "[!!] Computed field failed",
            ConfigErrorKind::Internal => "[!!] Internal configuration error",
        }
    }
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "configuration error ({:?})", self.kind)?;

        if let Some(key) = self.key() {
            write!(formatter, " for `{key}`")?;
        }
        if let Some(source) = self.source_name() {
            write!(formatter, " in `{source}`")?;
        }
        if let Some(expected) = self.expected_type() {
            write!(formatter, ", expected `{expected}`")?;
        }

        write!(formatter, ": {}", self.message)
    }
}

impl Error for ConfigError {}

/// An error returned by a custom [`crate::source::Source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    message: Box<str>,
}

impl SourceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into().into_boxed_str(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for SourceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_expose_error_context() {
        let error = ConfigError::invalid_value("APP_PORT", "u16", "invalid digit found in string");

        assert_eq!(error.kind(), ConfigErrorKind::InvalidValue);
        assert_eq!(error.key(), Some("APP_PORT"));
        assert_eq!(error.source_name(), None);
        assert_eq!(error.expected_type(), Some("u16"));
        assert!(error.message().contains("parse"));
    }

    #[test]
    fn display_includes_key_source_and_expected_type() {
        let error = ConfigError::invalid_value("APP_PORT", "u16", "oops");
        let text = error.to_string();

        assert!(text.contains("InvalidValue"));
        assert!(text.contains("`APP_PORT`"));
        assert!(text.contains("`u16`"));
        assert!(text.contains("oops"));
    }

    #[test]
    fn display_omits_absent_context() {
        let error = ConfigError::internal("boom");
        let text = error.to_string();

        assert!(!text.contains(" for `"));
        assert!(!text.contains(" in `"));
        assert!(!text.contains("expected"));
        assert!(text.ends_with("boom"));
    }

    #[test]
    fn safe_log_messages_never_leak_values() {
        let secret_reason = "secret-token";
        let error = ConfigError::invalid_value("APP_TOKEN", "String", secret_reason);

        assert!(!error.safe_log_message().contains(secret_reason));
    }

    #[test]
    fn computation_error_is_publicly_constructible() {
        let error = ConfigError::computation("public_url", "division by zero");

        assert_eq!(error.kind(), ConfigErrorKind::Computation);
        assert_eq!(error.key(), Some("public_url"));
        assert_eq!(error.source_name(), None);
        assert_eq!(error.expected_type(), None);
    }

    #[test]
    fn source_error_displays_its_message() {
        let error = SourceError::new("SNAFU disk on fire");
        assert_eq!(error.message(), "SNAFU disk on fire");
        assert_eq!(error.to_string(), "SNAFU disk on fire");
    }
}
