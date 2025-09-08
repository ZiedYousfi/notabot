use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Interpolate a template string by replacing tokens with values from `vars` and `globals`.
///
/// Supported token formats:
/// - `{{var_name}}` -> replaced with `vars["var_name"]` if present
/// - `{{@global_key}}` -> replaced with `globals["global_key"]` if present
///
/// Notes:
/// - Whitespace around the token content is ignored: `{{  var  }}` == `{{var}}`.
/// - Unknown tokens are left intact to aid debugging.
/// - Globals support dotted paths into JSON objects, e.g. `{{@app.name}}`.
/// - When a global is not a string, it is rendered as JSON (e.g., numbers as `42`, objects as `{"k":"v"}`).
pub fn interpolate_string(
    template: &str,
    vars: &HashMap<String, String>,
    globals: &BTreeMap<String, Value>,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut idx = 0;
    let bytes = template.as_bytes();

    while let Some(start) = find_subslice(bytes, b"{{", idx) {
        // Push everything up to the start of the token
        out.push_str(&template[idx..start]);

        // Find the end delimiter
        let content_start = start + 2;
        if let Some(end) = find_subslice(bytes, b"}}", content_start) {
            let raw = &template[content_start..end];
            let token = raw.trim();

            if token.is_empty() {
                // Keep empty tokens intact
                out.push_str(&template[start..end + 2]);
            } else {
                let replaced = if let Some(stripped) = token.strip_prefix('@') {
                    // Global lookup (supports dotted paths)
                    lookup_global(globals, stripped.trim()).unwrap_or_else(|| {
                        // Unknown -> keep original token
                        template[start..end + 2].to_string()
                    })
                } else {
                    // Variable lookup
                    vars.get(token)
                        .cloned()
                        .unwrap_or_else(|| template[start..end + 2].to_string())
                };
                out.push_str(&replaced);
            }

            idx = end + 2;
        } else {
            // No matching end, push rest and stop
            out.push_str(&template[start..]);
            idx = template.len();
            break;
        }
    }

    // Push any trailing text
    if idx < template.len() {
        out.push_str(&template[idx..]);
    }

    out
}

/// Interpolates all string values in a JSON structure (recursively).
///
/// - Strings are processed with `interpolate_string`.
/// - Arrays and objects are traversed recursively.
/// - Other types are returned unchanged.
pub fn interpolate_json(
    value: &Value,
    vars: &HashMap<String, String>,
    globals: &BTreeMap<String, Value>,
) -> Value {
    match value {
        Value::String(s) => Value::String(interpolate_string(s, vars, globals)),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| interpolate_json(v, vars, globals))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), interpolate_json(v, vars, globals));
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

/// Find the first occurrence of `needle` in `haystack` starting at `from`.
fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() {
        return None;
    }
    let end = haystack.len().saturating_sub(needle.len()) + 1;
    for i in from..end {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

/// Lookup a global value using a dotted path (e.g., "app.name").
/// Returns a string representation:
/// - If the final value is a JSON string, the contained string is returned.
/// - Otherwise, the value is serialized to compact JSON (e.g., numbers, objects).
fn lookup_global(globals: &BTreeMap<String, Value>, path: &str) -> Option<String> {
    let mut segments = path.split('.');

    // First segment is a top-level key in the globals map
    let first = segments.next()?.trim();
    let mut current = globals.get(first)?;

    for seg in segments {
        let seg = seg.trim();
        match current {
            Value::Object(map) => {
                current = map.get(seg)?;
            }
            _ => return None,
        }
    }

    match current {
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_interpolate_basic_vars() {
        let mut vars = HashMap::new();
        let globals = BTreeMap::new();

        vars.insert("name".into(), "Zied".into());
        vars.insert("greet".into(), "Hello".into());

        let t = "{{greet}}, {{name}}!";
        assert_eq!(interpolate_string(t, &vars, &globals), "Hello, Zied!");
    }

    #[test]
    fn test_interpolate_globals_string_and_number() {
        let vars = HashMap::new();
        let mut globals = BTreeMap::new();

        globals.insert("app".into(), json!("Notabot"));
        globals.insert("port".into(), json!(8080));

        let t = "Using {{@app}} on {{@port}}";
        assert_eq!(
            interpolate_string(t, &vars, &globals),
            "Using Notabot on 8080"
        );
    }

    #[test]
    fn test_interpolate_globals_dotted_path() {
        let vars = HashMap::new();
        let mut globals = BTreeMap::new();

        globals.insert(
            "app".into(),
            json!({
                "name": "Notabot",
                "meta": { "version": "0.1.0" }
            }),
        );

        assert_eq!(
            interpolate_string("{{@app.name}} v{{@app.meta.version}}", &vars, &globals),
            "Notabot v0.1.0"
        );
    }

    #[test]
    fn test_unknown_tokens_are_preserved() {
        let vars = HashMap::new();
        let globals = BTreeMap::new();

        let t = "Hello, {{name}} from {{@app}}!";
        assert_eq!(
            interpolate_string(t, &vars, &globals),
            "Hello, {{name}} from {{@app}}!"
        );
    }

    #[test]
    fn test_interpolate_json_recursive() {
        let mut vars = HashMap::new();
        let mut globals = BTreeMap::new();

        vars.insert("user".into(), "Alice".into());
        globals.insert("app".into(), json!("Notabot"));

        let v = json!({
            "msg": "Hi {{user}} from {{@app}}",
            "nested": {
                "arr": ["{{user}}", 1, true]
            }
        });

        let out = interpolate_json(&v, &vars, &globals);
        assert_eq!(
            out,
            json!({
                "msg": "Hi Alice from Notabot",
                "nested": { "arr": ["Alice", 1, true] }
            })
        );
    }
}
