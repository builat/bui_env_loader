use std::collections::HashMap;

use crate::source::Source;
use crate::SourceError;

/// I used in-memory map as a source for testing and for programmatic configuration here
/// to avoid the need for a file or process environment. It is not intended to be used in production.
/// Also .to_ascii_uppercase() is used to normalize keys to be case-insensitive, since environment variables are case-insensitive on Windows and case-sensitive on Linux. This is a common source of confusion when testing cross-platform code, so this source normalizes keys to be case-insensitive for consistency.
#[derive(Debug, Clone)]
pub struct MapSource {
    name: Box<str>,
    values: HashMap<String, String>,
}

impl MapSource {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().into_boxed_str(),
            values: HashMap::new(),
        }
    }

    /// Adds or replaces a value and returns `self` for builder-style use.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(key.into().to_ascii_uppercase(), value);
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values
            .insert(key.into().to_ascii_uppercase(), value.into());
    }
}

impl Source for MapSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        Ok(self.values.get(&key.to_ascii_uppercase()).cloned())
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(self.values.keys().cloned().collect()))
    }

    fn values(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(self.values.values().cloned().collect()))
    }
}
