use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;


/// Communication details for a device. Includes addresses and port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommunication {
    pub addresses: Vec<String>,
    pub port: u16,
}

/// CPU information of a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    #[serde(rename="clockSpeed")]
    pub clock_speed: HashMap<String, u64>, // name of the unit (e.g. Hz or MHZ) and its value (speed)
    #[serde(rename="coreCount")]
    pub core_count: u32,
    #[serde(rename="humanReadableName")]
    pub human_readable_name: String
}

/// Memory information of a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub bytes: u64 // Total memory in bytes
}

/// Single network interface ip information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceIpInfo {
    #[serde(rename="ipInfo")]
    pub ip_info: Vec<String> // List of IP addresses assigned to the interface, for example "192.168.1.1/24"
}

/// Information on the platforms os and kernel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    #[serde(rename="hostName")]
    pub host_name: String,
    pub kernel: String,
    pub name: String,
    pub os: String
}

/// Information on the platform hardware
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub network: HashMap<String, NetworkInterfaceIpInfo>, // name of the interface e.g. "eth0", followed by info on its assigned IPs
    pub system: OsInfo
}

/// Description of a device. Contains details of the hardware and os of the device,
/// as well as the different interfaces exposed by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDescription {
    pub platform: PlatformInfo,
    #[serde(rename = "supervisorInterfaces")]
    pub supervisor_interfaces: Vec<String>,
}

/// Represents the status of a device: active or inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusEnum {
    Active,
    Inactive,
}

/// Represents a single entry in the status log of a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusLogEntry {
    pub status: StatusEnum,
    pub time: chrono::DateTime<chrono::Utc>,
}

/// Represents a single healthreport from a device.
/// Contains the actual report as well as when the report was fetched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub report: HealthReport,
    // TODO: Uncomment this if you fix the time_of_query being stored as a string
    // #[serde(with = "chrono_datetime_as_bson_datetime")]
    pub time_of_query: chrono::DateTime<chrono::Utc>,
}

/// Network usage statistics for a single network interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceUsage {
    #[serde(rename="downBytes")]
    pub down_bytes: u64,     // Total bytes sent since last system start
    #[serde(rename="upBytes")]
    pub up_bytes: u64, // Total bytes received since last system start
}

/// The structure of a health report sent by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    #[serde(rename="cpuUsage")]
    pub cpu_usage: f32,       // CPU usage percentage
    #[serde(rename="memoryUsage")]
    pub memory_usage: f32,    // Memory usage percentage
    // TODO: Uncomment below once supervisor supports them
    // #[serde(rename="diskUsage")]
    // pub disk_usage: f32,      // Disk usage percentage
    // pub uptime: u64,          // Uptime in seconds
    #[serde(rename="networkUsage")]
    pub network_usage: HashMap<String, NetworkInterfaceUsage>, // Network usage per interface
}



/// Represents a device document from the "device" collection in MongoDB.
/// Note, the object id "_id" is not included here. Its meant to be fetched separate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub communication: DeviceCommunication,
    pub description: DeviceDescription,
    pub status: StatusEnum,
    pub ok_health_check_count: u32,
    pub failed_health_check_count: u32,
    pub status_log: Option<Vec<StatusLogEntry>>, // Optional, since status log may not have been generated yet
    pub health: Option<Health> // Optional, since health report may not have been fetched yet
}