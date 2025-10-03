use actix_web::{web, HttpResponse, Responder};
use serde_json::{Value, json};
use chrono::{DateTime, Utc};
use mongodb::bson::doc;
use crate::lib::mongodb::get_collection;
use futures::stream::TryStreamExt;
use log::{info, error};
use crate::lib::errors::ApiError;
use crate::lib::constants::COLL_NODE_CARDS;
use crate::structs::node_cards::NodeCard;


/// GET /nodeCards
/// 
/// Endpoint to create a node card
pub async fn create_node_card(card: web::Json<Value>) -> Result<impl Responder, ApiError> {
    info!("Received node card data: {:?}", card);

    // Extract the first asset from the asset array
    let asset = card.get("asset")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.get(0));
    if asset.is_none() {
        error!("Invalid metadata: Missing asset data");
        return Err(ApiError::bad_request("Invalid metadata: Missing asset data"));
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
        id: None,
        name: asset.get("title").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        nodeid: asset.get("uid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        zone,
        date_received: Utc::now(),
    };

    // Save the new card to MongoDB
    let collection = get_collection::<NodeCard>(COLL_NODE_CARDS).await;
    match collection.insert_one(&node_card).await {
        Ok(_) => Ok(HttpResponse::Ok().json(json!({ "message": "Node card received and saved", "nodeCard": node_card }))),
        Err(e) => {
            error!("Error creating node card: {}", e);
            Err(ApiError::internal_error("Error creating Node card"))
        }
    }
}


/// POST /nodeCards
/// 
/// Endpoint to get node cards
pub async fn get_node_cards(query: web::Query<std::collections::HashMap<String, String>>) -> Result<impl Responder, ApiError> {
    let collection = get_collection::<NodeCard>(COLL_NODE_CARDS).await;

    // Optional time filter
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        if let Ok(dt) = DateTime::parse_from_rfc3339(after) {
            let dt_utc = dt.with_timezone(&Utc);
            filter = doc! { "dateReceived": { "$gt": mongodb::bson::DateTime::from_chrono(dt_utc) } };
        }
    }

    // Get and return the results
    let cursor = match collection.find(filter).await {
        Ok(cursor) => cursor,
        Err(e) => {
            error!("Error querying node cards: {}", e);
            return Err(ApiError::internal_error("Error querying node cards"));
        }
    };
    let results: Vec<NodeCard> = match cursor.try_collect().await {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to collect node cards: {}", e);
            return Err(ApiError::db("Failed to collect node cards"));
        }
    };

    let mut v = serde_json::to_value(&results).map_err(ApiError::internal_error)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}


/// DELETE /nodeCards
/// 
/// Endpoint to delete all node cards
pub async fn delete_all_node_cards() -> Result<impl Responder, ApiError> {
    let collection = get_collection::<NodeCard>(COLL_NODE_CARDS).await;
    match collection.delete_many(doc! {}).await {
        Ok(result) => Ok(HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count }))),
        Err(e) => {
            error!("Failed to delete all node cards: {}", e);
            Err(ApiError::internal_error("Failed to delete node cards"))
        }
    }
}


/// DELETE /nodeCards/{card_id}
/// 
/// Endpoint to delete a specific node card by nodeid
pub async fn delete_node_card_by_id(path: web::Path<String>) -> Result<impl Responder, ApiError> {
    let nodeid = path.into_inner();
    let collection = get_collection::<NodeCard>(COLL_NODE_CARDS).await;
    match collection.delete_one(doc! { "nodeid": &nodeid }).await {
        Ok(result) => {
            if result.deleted_count == 1 {
                Ok(HttpResponse::Ok().json(json!({ "message": "Node card deleted", "nodeid": nodeid })))
            } else {
                Err(ApiError::not_found(format!("Node card not found, nodeid: {}", nodeid)))
            }
        }
        Err(e) => {
            error!("Failed to delete node card with nodeid {}: {}", nodeid, e);
            Err(ApiError::internal_error(format!("Failed to delete node card, nodeid: {}", nodeid)))
        }
    }
}
