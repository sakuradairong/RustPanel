//! Redact sensitive fields from structured audit payloads.
use serde_json::{Map, Value};

const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "authorization",
    "cookie",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
    "content", // file body may contain secrets
];

fn key_is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYS
        .iter()
        .any(|needle| lower == *needle || lower.contains(needle))
}

/// Recursively mask sensitive object keys; strings on sensitive keys become `[REDACTED]`.
pub fn mask_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(mask_map(map)),
        Value::Array(items) => Value::Array(items.into_iter().map(mask_value).collect()),
        other => other,
    }
}

fn mask_map(map: Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in map {
        if key_is_sensitive(&key) {
            out.insert(key, Value::String("[REDACTED]".into()));
        } else {
            out.insert(key, mask_value(value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_nested_password_and_token() {
        let masked = mask_value(json!({
            "user": "alice",
            "password": "hunter2",
            "nested": {"authorization": "Bearer x", "ok": 1},
            "content": "file-body"
        }));
        assert_eq!(masked["password"], "[REDACTED]");
        assert_eq!(masked["nested"]["authorization"], "[REDACTED]");
        assert_eq!(masked["nested"]["ok"], 1);
        assert_eq!(masked["content"], "[REDACTED]");
        assert_eq!(masked["user"], "alice");
    }
}
