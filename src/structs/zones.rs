use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime;
use mongodb::bson::oid::ObjectId;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zones {
    #[serde(rename="_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub zone: Option<String>,
    #[serde(rename = "allowedRiskLevels", skip_serializing_if="Option::is_none")]
    pub allowed_risk_levels: Option<Vec<String>>,
    #[serde(rename = "type", skip_serializing_if="Option::is_none")]
    pub r#type: Option<String>,
    #[serde(rename = "lastUpdated", with = "chrono_datetime_as_bson_datetime")]
    pub last_updated: DateTime<Utc>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub levels: Option<Vec<String>>
}
