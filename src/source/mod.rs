//! Built-in configuration sources and the extension trait for custom sources.

mod dotenv;
mod map;
mod process;

pub use map::MapSource;
pub use process::ProcessEnvSource;

pub(crate) use dotenv::{is_valid_key, DotenvFile, ParsedDotenv};

use crate::SourceError;

/// A named source of string values.
///
/// Type conversion does not belong here. A source only finds raw strings;
/// [`crate::ResolveSession`] performs the strongly typed `FromStr` conversion.
pub trait Source: Send + Sync {
    /// A short stable name used in reports, for example `process` or `.env`.
    fn name(&self) -> &str;

    fn get(&self, key: &str) -> Result<Option<String>, SourceError>;

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(None)
    }

    fn values(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(None)
    }

    fn entries(&self) -> Result<Option<Vec<(String, String)>>, SourceError> {
        let keys = match self.keys()? {
            Some(keys) => keys,
            None => return Ok(None),
        };

        let mut content_tuples = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(value) = self.get(&key)? {
                content_tuples.push((key, value));
            }
        }

        Ok(Some(content_tuples))
    }

    fn is_empty(&self) -> Result<bool, SourceError> {
        match self.keys()? {
            Some(keys) => Ok(keys.is_empty()),
            None => Ok(false),
        }
    }

    fn exists(&self, key: &str) -> Result<bool, SourceError> {
        match self.get(key)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}
