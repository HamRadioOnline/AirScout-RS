//! Table helpers of the database maintenance dialog (`ScoutBaseDatabase.cs:124-163`).
//!
//! A table name is never put into SQL unless it equals an entry of [`table_list`]; it is then
//! quoted as an identifier. Everything else is rejected.

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Error, Result};

/// The tables of the database, without `sqlite_*` (`GetTableList`), ordered by name.
pub fn table_list(conn: &Connection) -> Result<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
    )?;
    let rows = st.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// `table` as a quoted identifier, if it is a table of the database.
fn checked(conn: &Connection, table: &str) -> Result<String> {
    if table_list(conn)?.iter().any(|t| t == table) {
        Ok(format!("\"{}\"", table.replace('"', "\"\"")))
    } else {
        Err(Error::InvalidParameterName(format!(
            "unknown table: {table:?}"
        )))
    }
}

/// `TableRowCount`.
pub fn row_count(conn: &Connection, table: &str) -> Result<u64> {
    let t = checked(conn, table)?;
    let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?;
    Ok(n.max(0) as u64)
}

/// `ClearTable`: delete all rows in one transaction; returns how many.
pub fn clear_table(conn: &Connection, table: &str) -> Result<usize> {
    let t = checked(conn, table)?;
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute(&format!("DELETE FROM {t}"), [])?;
    tx.commit()?;
    Ok(n)
}

/// The first `limit` rows of the columns whose declared type is not BLOB, as text (read only).
pub fn select_non_blob(
    conn: &Connection,
    table: &str,
    limit: usize,
) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let t = checked(conn, table)?;
    let cols: Vec<String> = {
        let mut st = conn.prepare("SELECT name, type FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = st.query_map([table], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut v = Vec::new();
        for r in rows {
            let (name, ty) = r?;
            if !ty.eq_ignore_ascii_case("BLOB") {
                v.push(name);
            }
        }
        v
    };
    if cols.is_empty() {
        return Ok((cols, Vec::new()));
    }
    let list = cols
        .iter()
        .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let mut st = conn.prepare(&format!("SELECT {list} FROM {t} LIMIT ?1"))?;
    let n = cols.len();
    let rows = st.query_map([limit as i64], |r| {
        (0..n)
            .map(|i| {
                Ok(match r.get_ref(i)? {
                    ValueRef::Null => String::new(),
                    ValueRef::Integer(v) => v.to_string(),
                    ValueRef::Real(v) => v.to_string(),
                    ValueRef::Text(b) => String::from_utf8_lossy(b).into_owned(),
                    ValueRef::Blob(_) => "[blob]".into(),
                })
            })
            .collect::<Result<Vec<_>>>()
    })?;
    Ok((cols, rows.collect::<Result<_>>()?))
}
