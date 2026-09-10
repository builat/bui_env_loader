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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_normalized_to_upper_case() {
        let mut source = MapSource::new("test");
        source.insert("app_host", "localhost");

        assert_eq!(
            source.get("APP_HOST").unwrap().as_deref(),
            Some("localhost")
        );
        assert_eq!(
            source.get("app_host").unwrap().as_deref(),
            Some("localhost")
        );
        assert_eq!(
            source
                .with("MiXeD", "value")
                .get("mixed")
                .unwrap()
                .as_deref(),
            Some("value")
        );
    }

    #[test]
    fn later_inserts_replace_earlier_values() {
        let source = MapSource::new("test")
            .with("KEY", "first")
            .with("KEY", "second");

        assert_eq!(source.get("KEY").unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn default_trait_methods_work() {
        let empty = MapSource::new("empty");
        assert!(empty.is_empty().unwrap());
        assert!(!empty.exists("KEY").unwrap());
        assert_eq!(empty.entries().unwrap(), Some(Vec::new()));

        let source = MapSource::new("test").with("A", "1").with("B", "2");
        assert!(!source.is_empty().unwrap());
        assert!(source.exists("A").unwrap());
        assert!(!source.exists("C").unwrap());

        let mut entries = source.entries().unwrap().expect("map sources list entries");
        entries.sort();
        assert_eq!(
            entries,
            vec![
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "2".to_owned())
            ]
        );
    }

    #[test]
    fn missing_keys_return_none() {
        let source = MapSource::new("test").with("A", "1");
        assert_eq!(source.get("MISSING").unwrap(), None);
    }
}
