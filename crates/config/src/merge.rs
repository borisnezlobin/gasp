//! Layering user TOML over built-in defaults.

use toml::{Table, Value};

/// Merges `overlay` into `base`: tables merge key by key, anything else is replaced.
pub fn deep_merge(base: &mut Table, overlay: &Table) {
    for (key, value) in overlay {
        merge_value(base, key, value);
    }
}

fn merge_value(base: &mut Table, key: &str, value: &Value) {
    if let (Some(Value::Table(existing)), Value::Table(incoming)) = (base.get_mut(key), value) {
        deep_merge(existing, incoming);
        return;
    }
    base.insert(key.to_string(), value.clone());
}

/// Flattens nested tables into dotted keys. Empty tables stay as leaves.
pub fn flatten(table: &Table) -> Vec<(String, Value)> {
    let mut leaves = Vec::new();
    flatten_into(table, "", &mut leaves);
    leaves
}

fn flatten_into(table: &Table, prefix: &str, leaves: &mut Vec<(String, Value)>) {
    for (key, value) in table {
        let path = join_key(prefix, key);
        match value {
            Value::Table(inner) if !inner.is_empty() => flatten_into(inner, &path, leaves),
            _ => leaves.push((path, value.clone())),
        }
    }
}

/// Joins a dotted prefix and a key.
pub fn join_key(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn nested_tables_merge_and_scalars_replace() {
        let mut base = table("[a]\nx = 1\ny = 2\n[b]\nz = 3\n");
        deep_merge(&mut base, &table("[a]\ny = 20\nw = 4\n"));
        assert_eq!(base, table("[a]\nx = 1\ny = 20\nw = 4\n[b]\nz = 3\n"));
    }

    #[test]
    fn flatten_uses_dotted_keys() {
        let leaves = flatten(&table("[a.b]\nc = 1\n[d]\n"));
        let keys: Vec<_> = leaves.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["a.b.c", "d"]);
    }
}
