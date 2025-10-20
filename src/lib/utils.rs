use serde_json::Value;
use crate::structs::device::{DeviceDescription, PlatformInfo, CpuInfo, MemoryInfo, OsInfo};
use std::collections::HashMap;

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


/// Build a minimal placeholder description when a device hasn't reported one yet.
pub fn default_device_description() -> DeviceDescription {
    DeviceDescription {
        platform: PlatformInfo {
            cpu: CpuInfo {
                architecture: "unknown".to_owned(),
                clock_speed_hz: 0,
                core_count: 0,
                human_readable_name: String::new(),
            },
            memory: MemoryInfo { total_bytes: 0 },
            storage: HashMap::new(),
            network: HashMap::new(),
            system: OsInfo {
                host_name: String::new(),
                kernel: String::new(),
                name: String::new(),
                os: String::new(),
            },
        },
        supervisor_interfaces: Vec::new(),
    }
}
