use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime;
use mongodb::bson::oid::ObjectId;


/// Represents the structure of a node card stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCard {
    #[serde(rename="_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub nodeid: String,
    pub zone: String,
    #[serde(rename = "dateReceived", with = "chrono_datetime_as_bson_datetime")]
    pub date_received: DateTime<Utc>,
}