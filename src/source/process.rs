use std::collections::{HashMap, HashSet};
use std::env;

use crate::source::Source;
use crate::SourceError;

/// An immutable UTF-8 snapshot of the process environment.
///
/// Snapshotting gives a `load` or `materialize` operation deterministic input
/// and avoids mutating global process state. A value with non-UTF-8 bytes is
/// remembered and reported only if that exact key is requested.
#[derive(Debug, Clone)]
pub struct ProcessEnvSource {
    values: HashMap<String, String>,
    non_unicode_values: HashSet<String>,
}

impl ProcessEnvSource {
    pub fn snapshot() -> Self {
        let mut values = HashMap::new();
        let mut non_unicode_values = HashSet::new();

        for (raw_key, raw_value) in env::vars_os() {
            // A non-UTF-8 key cannot be requested through this crate's `&str`
            // API, so it is safe to ignore it.
            let Ok(key) = raw_key.into_string() else {
                continue;
            };

            match raw_value.into_string() {
                Ok(value) => {
                    values.insert(key, value);
                }
                Err(_) => {
                    // if value is not valid UTF-8, we still want to remember the key so that
                    // we can report an error if the user tries to request it.
                    non_unicode_values.insert(key);
                }
            }
        }

        Self {
            values,
            non_unicode_values,
        }
    }
}

impl Source for ProcessEnvSource {
    fn name(&self) -> &str {
        "process"
    }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        if self.non_unicode_values.contains(key) {
            return Err(SourceError::new(format!(
                "[!] The process variable is not valid UTF-8 for key: {}",
                key
            )));
        }

        Ok(self.values.get(key).cloned())
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        let mut keys: Vec<String> = self.values.keys().cloned().collect();
        keys.extend(self.non_unicode_values.iter().cloned());
        Ok(Some(keys))
    }

    fn entries(&self) -> Result<Option<Vec<(String, String)>>, SourceError> {
        let mut entries: Vec<(String, String)> = self
            .values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        for key in &self.non_unicode_values {
            entries.push((key.clone(), String::from("<non-UTF-8 value>")));
        }

        Ok(Some(entries))
    }

    fn is_empty(&self) -> Result<bool, SourceError> {
        Ok(self.values.is_empty() && self.non_unicode_values.is_empty())
    }

    fn exists(&self, key: &str) -> Result<bool, SourceError> {
        if self.non_unicode_values.contains(key) {
            return Ok(true);
        }
        Ok(self.values.contains_key(key))
    }

    fn values(&self) -> Result<Option<Vec<String>>, SourceError> {
        let mut values: Vec<String> = self.values.values().cloned().collect();
        for _ in &self.non_unicode_values {
            values.push(String::from("<non-UTF-8 value>"));
        }
        Ok(Some(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Distinct keys per test keep them independent of each other and of other
    // tests in the same binary, which share the process environment.
    #[test]
    fn snapshot_copies_the_process_environment() {
        const KEY: &str = "BUI_PROCESS_SOURCE_TEST_SNAPSHOT";
        env::set_var(KEY, "snapshot-value");
        let source = ProcessEnvSource::snapshot();

        assert_eq!(source.get(KEY).unwrap().as_deref(), Some("snapshot-value"));
        assert!(source.exists(KEY).unwrap());

        env::set_var(KEY, "changed-after-snapshot");
        assert_eq!(
            source.get(KEY).unwrap().as_deref(),
            Some("snapshot-value"),
            "the snapshot must not track later process mutations"
        );

        env::remove_var(KEY);
    }

    #[test]
    fn absent_keys_are_missing() {
        let source = ProcessEnvSource::snapshot();
        assert_eq!(source.get("BUI_PROCESS_SOURCE_TEST_MISSING").unwrap(), None);
        assert!(!source.exists("BUI_PROCESS_SOURCE_TEST_MISSING").unwrap());
    }

    #[test]
    fn keys_and_entries_stay_consistent() {
        const KEY: &str = "BUI_PROCESS_SOURCE_TEST_ENTRIES";
        env::set_var(KEY, "value");
        let source = ProcessEnvSource::snapshot();

        let keys = source.keys().unwrap().expect("process lists keys");
        assert!(keys.contains(&KEY.to_owned()));

        let entries = source.entries().unwrap().expect("process lists entries");
        assert!(entries.contains(&(KEY.to_owned(), "value".to_owned())));

        assert!(!source.is_empty().unwrap());
        env::remove_var(KEY);
    }

    #[test]
    fn source_name_is_stable() {
        assert_eq!(ProcessEnvSource::snapshot().name(), "process");
    }
}
