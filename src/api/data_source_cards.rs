use actix_web::{web, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use serde_json::Value;
use crate::lib::constants::COLL_DATASOURCE_CARDS;
use crate::lib::mongodb::get_collection;
use crate::structs::data_source_cards::DatasourceCard;
use crate::lib::errors::ApiError;
use log::{info, error};


/// POST /dataSourceCards
/// 
/// Takes a json document (odrl) and extracts relevant fields to create 
/// a new data source card for the device/node specified in the json document.
pub async fn create_data_source_card(card: web::Json<Value>) -> Result<impl Responder, ApiError> {
    info!("Received datasourcecard data: {:?}", card);

    // Extract the first item in "asset" array in the document.
    // This is assumed to contain the required information, other
    // items in the array are ignored.
    let asset = match card
        .get("asset")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.get(0))
    {
        Some(a) => a,
        None => {
            return Err(ApiError::bad_request("Invalid ODRL document: Asset not found"));
        }
    };

    // Get relation title from asset.title
    let name = asset
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Get the relations array from asset.relation
    let relations = asset
        .get("relation")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    // Helper function to pick a value from relations by its "type" field
    // and return its "value" field.
    let pick = |key: &str| -> Option<&str> {
        relations
            .iter()
            .find_map(|r| {
                let is_key = r
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == key)
                    .unwrap_or(false);
                if !is_key {
                    return None;
                }
                r.get("value").and_then(|v| v.as_str())
            })
    };

    // Extract required fields, with defaults if not found
    let ds_type = pick("type").unwrap_or("unknown").to_string();
    let risk_level = pick("risk-level").unwrap_or("unknown").to_string();
    let nodeid_str = pick("nodeid").unwrap_or("");

    // Check that the given nodeid is a valid ObjectId
    let nodeid = match ObjectId::parse_str(nodeid_str) {
        Ok(oid) => oid,
        Err(_) => {
            return Err(ApiError::bad_request("Invalid nodeid (expected ObjectId hex string)"));
        }
    };

    // Create the new DatasourceCard document and save it to database
    let doc = DatasourceCard {
        id: None,
        name,
        r#type: ds_type,
        risk_level,
        nodeid,
        date_received: Utc::now(),
    };
    let collection = get_collection::<DatasourceCard>(COLL_DATASOURCE_CARDS).await;
    match collection.insert_one(&doc).await {
        Ok(_) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Datasourcecard received and saved"
        }))),
        Err(e) => {
            error!("Error creating datasourcecard: {}", e);
            Err(ApiError::internal_error("Error creating datasourcecard"))
        }
    }
}


/// GET /dataSourceCards?after=<RFC3339>
/// 
/// Returns all data source cards. Can be given a date in RFC3339 format 
/// to get only entries greater than that date/time.
pub async fn get_data_source_card(
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<impl Responder, ApiError> {
    
    // Optional time filter
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        if let Ok(dt) = DateTime::parse_from_rfc3339(after) {
            let dt_utc = dt.with_timezone(&Utc);
            filter = doc! { "dateReceived": { "$gt": mongodb::bson::DateTime::from_chrono(dt_utc) } };
        }
    }

    // Query, collect and return the cards
    let collection = get_collection::<DatasourceCard>(COLL_DATASOURCE_CARDS).await;
    let cursor = match collection.find(filter).await {
        Ok(c) => c,
        Err(e) => {
            error!("Error querying data source cards: {}", e);
            return Err(ApiError::db("Error querying data source cards"));
        }
    };
    let results: Vec<DatasourceCard> = match cursor.try_collect().await {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to collect data source cards: {}", e);
            return Err(ApiError::db("Failed to collect data source cards"));
        }
    };
    let mut v = serde_json::to_value(&results).map_err(ApiError::internal_error)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}


/// DELETE /dataSourceCards
/// 
/// Deletes all data source cards.
pub async fn delete_all_data_source_cards() -> Result<impl Responder, ApiError> {
    let collection = get_collection::<DatasourceCard>(COLL_DATASOURCE_CARDS).await;
    match collection.delete_many(doc! {}).await {
        Ok(result) => {
            use serde_json::json;
            Ok(HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count })))
        }
        Err(e) => {
            error!("Failed to delete all data source cards: {}", e);
            Err(ApiError::internal_error("Failed to delete data source cards"))
        }
    }
}


/// DELETE /dataSourceCards/{node_id}
/// 
/// Deletes a single data source card by its nodeid.
pub async fn delete_data_source_card_by_nodeid(path: web::Path<String>) -> Result<impl Responder, ApiError> {

    // Convert the given nodeid string to ObjectId
    let nodeid_hex = path.into_inner();
    let nodeid = match ObjectId::parse_str(&nodeid_hex) {
        Ok(oid) => oid,
        Err(_) => {
            return Err(ApiError::bad_request("Invalid nodeid (expected ObjectId hex string)"));
        }
    };

    // Find the matching document and delete it if it exists
    let collection = get_collection::<DatasourceCard>(COLL_DATASOURCE_CARDS).await;
    match collection.delete_one(doc! { "nodeid": nodeid }).await {
        Ok(result) => {
            use serde_json::json;
            if result.deleted_count == 1 {
                Ok(HttpResponse::Ok().json(json!({
                    "message": "Data source card deleted",
                    "nodeid": nodeid_hex
                })))
            } else {
                Err(ApiError::not_found(format!("Data source card with nodeid {} not found", nodeid_hex)))
            }
        }
        Err(e) => {
            error!("Failed to delete data source card with nodeid {}: {}", nodeid_hex, e);
            Err(ApiError::db(format!("Failed to delete data source card with nodeid {}", nodeid_hex)))
        }
    }
}
