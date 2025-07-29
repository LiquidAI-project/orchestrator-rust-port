use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Document};
use crate::lib::mongodb::get_collection;
use futures::stream::TryStreamExt;

/// NodeCard structure
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCard {
    pub name: String,
    pub nodeid: String,
    pub zone: String,
    pub date_received: DateTime<Utc>,
}

/// Endpoint to create a node card
pub async fn create_node_card(card: web::Json<Value>) -> impl Responder {
    log::info!("Received node card data: {:?}", card);

    // Extract the first asset from the asset array
    let asset = card.get("asset")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.get(0));
    if asset.is_none() {
        log::error!("Invalid metadata: Missing asset data");
        return HttpResponse::BadRequest().json(json!({ "message": "Invalid metadata: Missing asset data" }));
    }
    let asset = asset.unwrap();

    // Extract zone information from relations
    let zone = asset.get("relation")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.iter().find(|rel| rel.get("type").and_then(|t| t.as_str()) == Some("memberOf")))
        .and_then(|rel| rel.get("value").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
        .to_string();

    // Create a new NodeCard instance
    let node_card = NodeCard {
        name: asset.get("title").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        nodeid: asset.get("uid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        zone,
        date_received: Utc::now(),
    };

    // Save the new card to MongoDB
    let doc = mongodb::bson::to_document(&node_card).unwrap();
    let collection = get_collection::<Document>("nodecards").await;
    match collection.insert_one(doc).await {
        Ok(_) => HttpResponse::Ok().json(json!({ "message": "Node card received and saved", "nodeCard": node_card })),
        Err(e) => {
            log::error!("Error creating node card: {}", e);
            HttpResponse::InternalServerError().json(json!({ "message": "Error creating Node card" }))
        }
    }
}

/// Endpoint to get node cards
pub async fn get_node_cards(query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {
    let collection = get_collection::<Document>("nodecards").await;

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
            log::error!("Error querying node cards: {}", e);
            return HttpResponse::InternalServerError().json(json!({ "message": "Error querying node cards" }));
        }
    };
    let mut results = Vec::new();
    while let Some(doc) = cursor.try_next().await.unwrap_or(None) {
        let card: NodeCard = match mongodb::bson::from_document(doc) {
            Ok(card) => card,
            Err(e) => {
                log::warn!("Failed to deserialize node card: {}", e);
                continue;
            }
        };
        results.push(card);
    }

    HttpResponse::Ok().json(results)
}

/// Endpoint to delete all node cards
pub async fn delete_all_node_cards() -> impl Responder {
    let collection = get_collection::<Document>("nodecards").await;
    match collection.delete_many(doc! {}).await {
        Ok(result) => HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count })),
        Err(e) => {
            log::error!("Failed to delete all node cards: {}", e);
            HttpResponse::InternalServerError().json(json!({ "message": "Failed to delete node cards" }))
        }
    }
}

/// Endpoint to delete a specific node card by nodeid
pub async fn delete_node_card_by_id(path: web::Path<String>) -> impl Responder {
    let nodeid = path.into_inner();
    let collection = get_collection::<Document>("nodecards").await;
    match collection.delete_one(doc! { "nodeid": &nodeid }).await {
        Ok(result) => {
            if result.deleted_count == 1 {
                HttpResponse::Ok().json(json!({ "message": "Node card deleted", "nodeid": nodeid }))
            } else {
                HttpResponse::NotFound().json(json!({ "message": "Node card not found", "nodeid": nodeid }))
            }
        }
        Err(e) => {
            log::error!("Failed to delete node card with nodeid {}: {}", nodeid, e);
            HttpResponse::InternalServerError().json(json!({ "message": "Failed to delete node card", "nodeid": nodeid }))
        }
    }
}
