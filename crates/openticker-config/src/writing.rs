use crate::error::ConfigError;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use toml_edit::DocumentMut;

/// Renders an updated TOML document by merging value changes from `next` into
/// `existing_raw`, preserving comments and formatting on untouched keys.
///
/// Only keys whose value differs are rewritten. The raw text is round-tripped
/// through `T` before comparison so serde defaults participate: a key absent on
/// disk whose effective value did not change stays absent. Keys present on disk
/// but absent from `next` are removed. Arrays — including arrays of tables such
/// as `[[indicators]]` — are replaced wholesale when their value differs.
///
/// # Errors
///
/// Returns [`ConfigError::Render`] when `existing_raw` is not valid TOML for `T`
/// or when `next` cannot be serialized to TOML.
pub fn render_updated_document<T>(existing_raw: &str, next: &T) -> Result<String, ConfigError>
where
    T: Serialize + DeserializeOwned,
{
    let mut document: DocumentMut = existing_raw
        .parse()
        .map_err(|error: toml_edit::TomlError| ConfigError::render(error.to_string()))?;
    let current: T =
        toml::from_str(existing_raw).map_err(|error| ConfigError::render(error.to_string()))?;
    let current_table = serialize_to_table(&current)?;
    let next_table = serialize_to_table(next)?;

    apply_table_diff(document.as_table_mut(), &current_table, &next_table);

    Ok(document.to_string())
}

/// Renders a brand-new TOML document for `value`.
///
/// # Errors
///
/// Returns [`ConfigError::Render`] when `value` cannot be serialized to TOML.
pub fn render_new_document<T: Serialize>(value: &T) -> Result<String, ConfigError> {
    toml::to_string_pretty(value).map_err(|error| ConfigError::render(error.to_string()))
}

/// Atomically replaces `path` with `contents`.
///
/// The contents are written to a temp file named `.{filename}.tmp-{pid}` in the
/// same directory, fsynced, and renamed over the target so readers never observe
/// a partially written file. The leading dot and non-`.toml` suffix keep
/// directory scanners and file watchers from picking the temp file up.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] when the temp file cannot be written or the
/// rename over the target fails.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), ConfigError> {
    let Some(file_name) = path.file_name() else {
        return Err(ConfigError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name"),
        });
    };

    let directory = path.parent().unwrap_or_else(|| Path::new(""));
    let temp_path = directory.join(format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));

    if let Err(source) = write_and_sync(&temp_path, contents) {
        let _ = fs::remove_file(&temp_path);
        return Err(ConfigError::Io {
            path: temp_path,
            source,
        });
    }

    if let Err(source) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(ConfigError::Io {
            path: path.to_path_buf(),
            source,
        });
    }

    Ok(())
}

fn write_and_sync(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

fn serialize_to_table<T: Serialize>(value: &T) -> Result<toml::Table, ConfigError> {
    toml::Table::try_from(value).map_err(|error| ConfigError::render(error.to_string()))
}

/// Applies the value-level difference between `current` and `next` onto
/// `document`, touching only keys whose value changed.
fn apply_table_diff(
    document: &mut dyn toml_edit::TableLike,
    current: &toml::Table,
    next: &toml::Table,
) {
    for key in current.keys() {
        if !next.contains_key(key) {
            document.remove(key);
        }
    }

    for (key, next_value) in next {
        if current.get(key) == Some(next_value) {
            continue;
        }
        if let (Some(toml::Value::Table(current_child)), toml::Value::Table(next_child)) =
            (current.get(key), next_value)
            && let Some(child) = document
                .get_mut(key)
                .and_then(toml_edit::Item::as_table_like_mut)
        {
            apply_table_diff(child, current_child, next_child);
            continue;
        }
        document.insert(key, value_to_item(next_value));
    }
}

fn value_to_item(value: &toml::Value) -> toml_edit::Item {
    match value {
        toml::Value::Table(table) => toml_edit::Item::Table(table_to_edit_table(table)),
        toml::Value::Array(values) if is_array_of_tables(values) => {
            let mut array = toml_edit::ArrayOfTables::new();
            for entry in values {
                if let toml::Value::Table(table) = entry {
                    array.push(table_to_edit_table(table));
                }
            }
            toml_edit::Item::ArrayOfTables(array)
        }
        other => toml_edit::Item::Value(value_to_edit_value(other)),
    }
}

fn is_array_of_tables(values: &[toml::Value]) -> bool {
    !values.is_empty() && values.iter().all(toml::Value::is_table)
}

fn table_to_edit_table(table: &toml::Table) -> toml_edit::Table {
    let mut out = toml_edit::Table::new();
    for (key, value) in table {
        out.insert(key, value_to_item(value));
    }
    out
}

fn value_to_edit_value(value: &toml::Value) -> toml_edit::Value {
    match value {
        toml::Value::String(text) => text.clone().into(),
        toml::Value::Integer(number) => (*number).into(),
        toml::Value::Float(number) => (*number).into(),
        toml::Value::Boolean(flag) => (*flag).into(),
        toml::Value::Datetime(datetime) => (*datetime).into(),
        toml::Value::Array(values) => values
            .iter()
            .map(value_to_edit_value)
            .collect::<toml_edit::Array>()
            .into(),
        toml::Value::Table(table) => {
            let mut inline = toml_edit::InlineTable::new();
            for (key, entry) in table {
                inline.insert(key, value_to_edit_value(entry));
            }
            inline.into()
        }
    }
}
