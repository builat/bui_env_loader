use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::source::Source;
use crate::{ConfigError, SourceError};

#[derive(Debug, Clone)]
pub(crate) struct DotenvFile {
    pub path: PathBuf,
    pub required: bool,
}

#[derive(Debug)]
pub(crate) struct ParsedDotenv {
    pub source: DotenvSource,
    pub duplicate_keys: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct DotenvSource {
    // Box<str> is used here to avoid the extra allocation of a String when the source name is used in error messages.
    name: Box<str>,
    values: HashMap<String, String>,
}

impl ParsedDotenv {
    pub fn from_path(path: &Path) -> Result<ParsedDotenv, ConfigError> {
        let display_name: String = path.display().to_string();
        let text: String = fs::read_to_string(path)
            .map_err(|error: std::io::Error| ConfigError::io(display_name.clone(), error))?;

        let (values, duplicate_keys) = parse(&text, &display_name)?;

        Ok(ParsedDotenv {
            source: DotenvSource {
                name: display_name.into_boxed_str(),
                values,
            },
            duplicate_keys,
        })
    }
}

impl Source for DotenvSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        Ok(self.values.get(key).cloned())
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(self.values.keys().cloned().collect()))
    }

    fn values(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(self.values.values().cloned().collect()))
    }

    fn entries(&self) -> Result<Option<Vec<(String, String)>>, SourceError> {
        let entries: Vec<(String, String)> = self
            .values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        Ok(Some(entries))
    }

    fn exists(&self, key: &str) -> Result<bool, SourceError> {
        Ok(self.values.contains_key(key))
    }

    fn is_empty(&self) -> Result<bool, SourceError> {
        Ok(self.values.is_empty())
    }
}

/// Parses the deliberately small dotenv grammar supported by version 0.1.
///
/// Supported features:
/// - `KEY=value` and `export KEY=value`;
/// - empty values;
/// - single and double quoted values;
/// - common escapes inside double quotes;
/// - comments outside quoted strings.
///
/// Shell interpolation and multiline values are intentionally not supported for the first versions.
fn parse(
    input: &str,
    source_name: &str,
) -> Result<(HashMap<String, String>, Vec<String>), ConfigError> {
    let mut values = HashMap::new();
    let mut duplicates = Vec::new();

    for (idx, original_line) in input.lines().enumerate() {
        let line_number = idx + 1;
        let mut line = original_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(rest) = line.strip_prefix("export") {
            // `exported=value` is a normal key; `export KEY=value` uses the
            // optional dotenv keyword.
            if rest.starts_with(char::is_whitespace) {
                line = rest.trim_start();
            }
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(dotenv_line_error(
                source_name,
                line_number,
                "[!] Expected `KEY=value`",
            ));
        };

        let key = raw_key.trim();
        if !is_valid_key(key) {
            return Err(dotenv_line_error(
                source_name,
                line_number,
                "[!] Invalid environment variable name",
            ));
        }

        let value = parse_value(raw_value, source_name, line_number)?;
        if values.insert(key.to_owned(), value).is_some() {
            duplicates.push(key.to_owned());
        }
    }

    Ok((values, duplicates))
}

fn parse_value(raw: &str, source_name: &str, line_number: usize) -> Result<String, ConfigError> {
    let raw = raw.trim_start();

    match raw.chars().next() {
        Some('\'') => parse_quoted(raw, '\'', source_name, line_number),
        Some('"') => parse_quoted(raw, '"', source_name, line_number),
        _ => Ok(parse_unquoted(raw)),
    }
}

fn parse_quoted(
    raw: &str,
    quote: char,
    source_name: &str,
    line_number: usize,
) -> Result<String, ConfigError> {
    let mut result = String::new();
    let mut escaped = false;
    let mut closing_index = None;

    // `char_indices().skip(1)` is safe here because the caller already checked
    // that the first Unicode scalar is the opening quote.
    for (idx, character) in raw.char_indices().skip(1) {
        if quote == '"' && escaped {
            let decoded = match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                other => other,
            };
            result.push(decoded);
            escaped = false;
            continue;
        }

        if quote == '"' && character == '\\' {
            escaped = true;
            continue;
        }

        if character == quote {
            closing_index = Some(idx + character.len_utf8());
            break;
        }

        result.push(character);
    }

    if escaped || closing_index.is_none() {
        return Err(dotenv_line_error(
            source_name,
            line_number,
            "[!] Unterminated quoted value",
        ));
    }

    let rest = raw[closing_index.expect("checked above")..].trim();
    if !rest.is_empty() && !rest.starts_with('#') {
        return Err(dotenv_line_error(
            source_name,
            line_number,
            "[!] Unexpected characters after quoted value",
        ));
    }

    Ok(result)
}

fn parse_unquoted(raw: &str) -> String {
    let mut end = raw.len();
    let mut previous_was_whitespace = true;

    for (index, character) in raw.char_indices() {
        if character == '#' && previous_was_whitespace {
            end = index;
            break;
        }
        previous_was_whitespace = character.is_whitespace();
    }

    raw[..end].trim_end().to_owned()
}

pub(crate) fn is_valid_key(key: &str) -> bool {
    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn dotenv_line_error(source_name: &str, line_number: usize, message: &str) -> ConfigError {
    ConfigError::dotenv(source_name, format!("line {line_number}: {message}"))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_supported_dotenv_syntax() {
        let input = r#"
            # comment
            HOST=127.0.0.1
            export PORT=8080
            EMPTY=
            MESSAGE="hello\nworld"
            LITERAL='a # is not a comment'
            URL=https://example.test/#fragment
            COMMENTED=value # comment
        "#;

        let (values, duplicates) = parse(input, "test.env").unwrap();

        assert_eq!(values["HOST"], "127.0.0.1");
        assert_eq!(values["PORT"], "8080");
        assert_eq!(values["EMPTY"], "");
        assert_eq!(values["MESSAGE"], "hello\nworld");
        assert_eq!(values["LITERAL"], "a # is not a comment");
        assert_eq!(values["URL"], "https://example.test/#fragment");
        assert_eq!(values["COMMENTED"], "value");
        assert!(duplicates.is_empty());
    }

    #[test]
    fn reports_duplicate_keys_but_keeps_last_value() {
        let (values, duplicates) = parse("PORT=80\nPORT=8080\n", "test.env").unwrap();

        assert_eq!(values["PORT"], "8080");
        assert_eq!(duplicates, vec!["PORT"]);
    }
}
