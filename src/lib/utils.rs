use serde_json::Value;

/// Recursively converts Extended JSON ObjectId objects {"$oid":"…"} into plain strings "…"
/// (Mongodb returns ObjectsIds in a format that frontend doesnt know how to handle, this fixes that)
pub fn normalize_object_ids(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(v) = map.get("$oid") {
                    if let Some(s) = v.as_str() {
                        *value = Value::String(s.to_string());
                        return;
                    }
                }
            }
            for v in map.values_mut() {
                normalize_object_ids(v);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                normalize_object_ids(v);
            }
        }
        _ => {}
    }
}
