use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Document};
use crate::lib::mongodb::get_collection;
use futures::stream::TryStreamExt;

/// Datasourcecard structure
#[derive(Debug, Serialize, Deserialize)]
pub struct DataSourceCard {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(rename = "risk-level")]
    pub risk_level: String,
    pub nodeid: String,
    pub date_received: DateTime<Utc>,
}

/// Endpoint to create a datasourcecard
pub async fn create_data_source_card(card: web::Json<Value>) -> impl Responder {
    log::info!("Received datasourcecard data: {:?}", card);

    // Extract the necessary information for the datasourcecard
    let asset = card.get("asset")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.get(0));
    if asset.is_none() {
        return HttpResponse::BadRequest().json(serde_json::json!({ "message": "Invalid ODRL document: Asset not found" }));
    }
    let asset = asset.unwrap();
    let name = asset.get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let relations = asset.get("relation")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let type_ = relations.iter()
        .find_map(|r| r.get("type").and_then(|t| t.as_str()).filter(|&t| t == "type")
            .and_then(|_| r.get("value").and_then(|v| v.as_str())))
        .unwrap_or("unknown")
        .to_string();
    let risk_level = relations.iter()
        .find_map(|r| r.get("type").and_then(|t| t.as_str()).filter(|&t| t == "risk-level")
            .and_then(|_| r.get("value").and_then(|v| v.as_str())))
        .unwrap_or("unknown")
        .to_string();
    let nodeid = relations.iter()
        .find_map(|r| r.get("type").and_then(|t| t.as_str()).filter(|&t| t == "nodeid")
            .and_then(|_| r.get("value").and_then(|v| v.as_str())))
        .unwrap_or("unknown")
        .to_string();

    // Create and save the datasourcecard based on earlier extracted information
    let parsed_data_source = DataSourceCard {
        name,
        type_,
        risk_level,
        nodeid,
        date_received: Utc::now(),
    };
    let doc = mongodb::bson::to_document(&parsed_data_source).unwrap();
    let collection = get_collection::<Document>("datasourcecards").await;
    match collection.insert_one(doc).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "message": "Datasourcecard received and saved" })),
        Err(e) => {
            log::error!("Error creating datasourcecard: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({ "message": "Error creating datasourcecard" }))
        }
    }
}

/// Endpoint to retrieve datasourcecards
pub async fn get_data_source_card(query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {
    let collection = get_collection::<Document>("datasourcecards").await;

    // Ensure index on date_received
    let index_model = mongodb::IndexModel::builder().keys(doc! { "date_received": 1 }).build();
    let _ = collection.create_index(index_model).await;

    // Create a filter if query parameters were given
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        if let Ok(after_date) = DateTime::parse_from_rfc3339(after) {
            filter = doc! { "date_received": { "$gt": mongodb::bson::DateTime::from_millis(after_date.timestamp_millis()) } };
        }
    }

    // Get and return the results
    let mut cursor = match collection.find(filter).await {
        Ok(cursor) => cursor,
        Err(e) => {
            log::error!("Error querying data source cards: {}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({ "message": "Error querying data source cards" }));
        }
    };
    let mut results = Vec::new();
    while let Some(doc) = cursor.try_next().await.unwrap_or(None) {
        let card: DataSourceCard = match mongodb::bson::from_document(doc) {
            Ok(card) => card,
            Err(e) => {
                log::warn!("Failed to deserialize data source card: {}", e);
                continue;
            }
        };
        results.push(card);
    }
    HttpResponse::Ok().json(results)
}

/// Endpoint to delete all datasourcecards
pub async fn delete_all_data_source_cards() -> impl Responder {
    let collection = get_collection::<Document>("datasourcecards").await;
    match collection.delete_many(doc! {}).await {
        Ok(result) => HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count })),
        Err(e) => {
            log::error!("Failed to delete all data source cards: {}", e);
            HttpResponse::InternalServerError().json(json!({ "message": "Failed to delete data source cards" }))
        }
    }
}

/// Endpoint to delete a specific datasourcecard by nodeid
pub async fn delete_data_source_card_by_nodeid(path: web::Path<String>) -> impl Responder {
    let nodeid = path.into_inner();
    let collection = get_collection::<Document>("datasourcecards").await;
    match collection.delete_one(doc! { "nodeid": &nodeid }).await {
        Ok(result) => {
            if result.deleted_count == 1 {
                HttpResponse::Ok().json(json!({ "message": "Data source card deleted", "nodeid": nodeid }))
            } else {
                HttpResponse::NotFound().json(json!({ "message": "Data source card not found", "nodeid": nodeid }))
            }
        }
        Err(e) => {
            log::error!("Failed to delete data source card with nodeid {}: {}", nodeid, e);
            HttpResponse::InternalServerError().json(json!({ "message": "Failed to delete data source card", "nodeid": nodeid }))
        }
    }
}
