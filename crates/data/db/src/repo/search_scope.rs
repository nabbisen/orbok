//! SQL for [`orbok_core::SearchScope`] (RFC-060 §7).
//!
//! One builder, used by all three retrieval queries, so a scope means the
//! same thing to the keyword, trigram and vector paths. Every value is
//! bound, never interpolated.

use orbok_core::SearchScope;
use rusqlite::types::Value;

/// A predicate to append inside a `WHERE`, with the values it binds.
pub struct ScopeSql {
    /// SQL beginning with `AND `, or empty for an unrestricted scope.
    pub predicate: String,
    pub binds: Vec<Value>,
}

/// Build the predicate. `files` and `sources` are the table aliases in the
/// caller's query; `next_param` is the first free `?n` index.
///
/// "Folder only" is expressed as "nothing left after the folder's own path
/// but a file name": the separator is bound from the running platform
/// rather than assumed to be `/`, since canonical paths on Windows use
/// `\`.
pub fn scope_sql(scope: &SearchScope, files: &str, sources: &str, next_param: usize) -> ScopeSql {
    let mut predicate = String::new();
    let mut binds: Vec<Value> = Vec::new();
    let mut param = next_param;

    if !scope.extensions.is_empty() {
        let placeholders: Vec<String> = scope
            .extensions
            .iter()
            .map(|ext| {
                binds.push(Value::Text(ext.to_ascii_lowercase()));
                let p = format!("?{param}");
                param += 1;
                p
            })
            .collect();
        predicate.push_str(&format!(
            " AND lower({files}.extension) IN ({})",
            placeholders.join(",")
        ));
    }

    if let Some(folder) = &scope.folder {
        predicate.push_str(&format!(" AND {files}.source_id = ?{param}"));
        binds.push(Value::Text(folder.source_id.clone()));
        param += 1;
        if !folder.include_subfolders {
            predicate.push_str(&format!(
                " AND instr(substr({files}.canonical_path, length({sources}.canonical_path) + 2), \
                 ?{param}) = 0"
            ));
            binds.push(Value::Text(std::path::MAIN_SEPARATOR.to_string()));
        }
    }

    ScopeSql { predicate, binds }
}
