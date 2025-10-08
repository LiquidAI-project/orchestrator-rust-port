use bson::oid::ObjectId;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};


/// Structure for the supervisor log data, this is the format its saved into database as
#[derive(Debug, Serialize, Deserialize)]
pub struct SupervisorLog {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(rename = "deviceIP")]
    pub device_ip: String,
    #[serde(rename = "deviceName")]
    pub device_name: String,
    #[serde(rename = "funcName")]
    pub func_name: String,
    #[serde(rename = "loglevel")]
    pub log_level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "dateReceived", with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub date_received: DateTime<Utc>, // Timestamp of when this log was received by the orchestrator
}