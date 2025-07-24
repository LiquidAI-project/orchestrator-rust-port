use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use mongodb::bson::{self, doc, oid::ObjectId, Document};
use actix_web::{web, HttpResponse, Responder};
use crate::lib::mongodb::{get_collection};
use futures::stream::TryStreamExt;
use actix_web::web::Form;

#[derive(Debug, Serialize, Deserialize)]
pub struct SupervisorLog {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(rename = "dateReceived")]
    pub date_received: DateTime<Utc>,
    
    #[serde(flatten)]
    pub content: serde_json::Value, // dynamic keys from logData
}

pub async fn post_supervisor_log(form: Form<std::collections::HashMap<String, String>>) -> impl Responder {
    if let Some(log_data_str) = form.get("logData") {
        let mut log_data: Value = match serde_json::from_str(log_data_str) {
            Ok(val) => val,
            Err(e) => {
                log::error!("Failed to parse logData as JSON: {}", e);
                return HttpResponse::BadRequest().body("Invalid logData JSON");
            }
        };
        log_data["dateReceived"] = json!(Utc::now());
        log::debug!("Received supervisor log: {:?}", log_data);
        let doc: Document = bson::to_document(&log_data).unwrap();
        let collection = get_collection::<Document>("supervisorLogs").await;

        match collection.insert_one(doc).await {
            Ok(_) => HttpResponse::Ok().json(json!({ "message": "Log received and saved" })),
            Err(e) => {
                log::error!("❌ Failed to insert supervisor log: {}", e);
                HttpResponse::InternalServerError().body("Log not saved")
            }
        }
    } else {
        HttpResponse::BadRequest().body("Missing logData field")
    }
}


pub async fn get_supervisor_logs(query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {
    let after_filter = query.get("after")
        .and_then(|after| DateTime::parse_from_rfc3339(after).ok())
        .map(|dt| bson::DateTime::from_millis(dt.with_timezone(&Utc).timestamp_millis()));

    let filter = if let Some(after) = after_filter {
        doc! { "dateReceived": { "$gt": after } }
    } else {
        doc! {}
    };

    let collection = get_collection::<Document>("supervisorLogs").await;

    match collection.find(filter).await {
        Ok(cursor) => {
            let logs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
            HttpResponse::Ok().json(logs)
        }
        Err(e) => {
            log::error!("❌ Failed to fetch supervisor logs: {}", e);
            HttpResponse::InternalServerError().body("Failed to fetch logs")
        }
    }
}

