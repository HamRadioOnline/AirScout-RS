//! Audit independently derived deviations from the C# fixtures.

use crate::{num, repo_root};
use serde_json::Value;
use std::collections::BTreeMap;

struct Row {
    csharp: String,
    expected: Value,
    findings: String,
}

/// One group of documented overrides. Every entry must be exercised and
/// every reference value outside the manifest must still match C#.
pub struct DeltaManifest {
    rows: BTreeMap<(usize, String), Row>,
}

/// Compare a JSON value recursively, applying the tolerance to numeric fields.
pub fn equivalent(a: &Value, b: &Value, tol: f64) -> bool {
    if let (Some(a), Some(b)) = (num(a), num(b)) {
        return (a.is_nan() && b.is_nan()) || a == b || (a - b).abs() <= tol;
    }
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b, tol))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| equivalent(v, w, tol)))
        }
        _ => a == b,
    }
}

impl DeltaManifest {
    pub fn load(group: &str) -> Self {
        Self::load_from("docs/logic-fixes-area1/golden-deltas.tsv", group)
    }

    pub fn load_from(path: &str, group: &str) -> Self {
        let path = repo_root().join(path);
        let text = std::fs::read_to_string(path).expect("read documented golden deltas");
        let mut rows = BTreeMap::new();
        for line in text.lines().skip(1) {
            let parts: Vec<_> = line.split('\t').collect();
            let [entry_group, index, field, csharp, expected, findings] = parts.as_slice() else {
                panic!("invalid golden delta row: {line}");
            };
            if *entry_group == group {
                let row = Row {
                    csharp: csharp.to_string(),
                    expected: serde_json::from_str(expected).expect("JSON golden value"),
                    findings: findings.to_string(),
                };
                let key = (index.parse().expect("case index"), field.to_string());
                assert!(rows.insert(key, row).is_none(), "duplicate entry: {line}");
            }
        }
        Self { rows }
    }

    #[track_caller]
    pub fn expected(
        &mut self,
        index: usize,
        field: &str,
        csharp: &Value,
        reference: Value,
        tolerance: f64,
        findings: &str,
    ) -> Value {
        let unchanged = equivalent(csharp, &reference, tolerance);
        match self.rows.remove(&(index, field.to_owned())) {
            Some(row) => {
                assert!(!unchanged, "case {index} {field} still matches C#");
                assert_eq!(row.csharp, csharp.to_string(), "case {index} {field}");
                assert_eq!(row.findings, findings, "case {index} {field}");
                assert!(
                    equivalent(&row.expected, &reference, tolerance),
                    "case {index} {field}: documented {} vs independent reference {reference}",
                    row.expected
                );
                reference
            }
            None => {
                assert!(
                    unchanged,
                    "case {index} {field}: missing golden delta {csharp} → {reference} ({findings})"
                );
                csharp.clone()
            }
        }
    }

    pub fn number(
        &mut self,
        index: usize,
        field: &str,
        csharp: &Value,
        reference: f64,
        tolerance: f64,
        findings: &str,
    ) -> f64 {
        let value = if reference.is_nan() {
            Value::from("NaN")
        } else if reference == f64::INFINITY {
            Value::from("Infinity")
        } else if reference == f64::NEG_INFINITY {
            Value::from("-Infinity")
        } else {
            Value::from(reference)
        };
        num(&self.expected(index, field, csharp, value, tolerance, findings)).unwrap()
    }

    pub fn finish(self) {
        assert!(
            self.rows.is_empty(),
            "unexercised golden deltas: {:?}",
            self.rows.keys()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::equivalent;
    use serde_json::json;

    #[test]
    fn equivalent_applies_tolerance_inside_objects_and_arrays() {
        // 45.05269849724853 and 45.052698497248535 differ in the last ulp only
        // (x86_64 vs aarch64 libm); a composite must compare like its scalars.
        let a = json!({"plane": {"track": 45.05269849724853, "hex": "3C0001"}, "inserted": [1]});
        let b = json!({"plane": {"track": 45.052698497248535, "hex": "3C0001"}, "inserted": [1]});
        assert!(equivalent(&a, &b, 1e-9));
        let c = json!({"plane": {"track": 45.1, "hex": "3C0001"}, "inserted": [1]});
        assert!(!equivalent(&a, &c, 1e-9));
        assert!(!equivalent(&json!([1, 2]), &json!([1]), 1e-9));
        assert!(!equivalent(&json!({"a": 1}), &json!({"b": 1}), 1e-9));
        assert!(!equivalent(&json!({"a": "x"}), &json!({"a": "y"}), 1e-9));
    }
}
