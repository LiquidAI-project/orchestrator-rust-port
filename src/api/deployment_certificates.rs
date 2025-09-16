use chrono::Utc;
use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{self, doc};
use serde_json;
use futures::TryStreamExt;
use crate::api::module_cards::ModuleCard;
use crate::lib::mongodb::{get_collection, find_one, insert_one};
use crate::structs::data_source_cards::DatasourceCard;
use actix_web::{self, HttpResponse, Responder};
use crate::structs::node_cards::NodeCard;
use crate::structs::zones::Zones;
use crate::api::deployment::CreateSolutionResult;
use crate::api::deployment::ApiError;
use crate::structs::deployment_certificates::{
    ValidationLog,
    DeploymentCertificate
};


/// Validates that a given deployment fulfills all the different constraints set to it
/// via different cards, like node cards, device cards and module cards.
pub async fn validate_deployment_solution(
    deployment_id: &ObjectId,
    solution: &CreateSolutionResult,
) -> Result<(), String> {

    let zones_coll = get_collection::<Zones>("zones").await;
    let mut zone_allowed: HashMap<String, Vec<String>> = HashMap::new();
    let mut cursor = zones_coll
        .find(doc! {})
        .await
        .map_err(|e| format!("zones.find error: {e}"))?;
    while let Some(z) = cursor
        .try_next()
        .await
        .map_err(|e| format!("zones cursor error: {e}"))?
    {
        if let Some(name) = z.zone.clone() {
            zone_allowed.insert(name, z.allowed_risk_levels.unwrap_or_default());
        }
    }

    let mut output_risk = "none".to_string();
    let mut logs: Vec<ValidationLog> = Vec::new();

    for step in &solution.sequence {
        let device_hex = step.device.to_hex();
        let module_hex = step.module.to_hex();

        let mut log = ValidationLog {
            device: device_hex.clone(),
            module: module_hex.clone(),
            func: step.func.clone(),
            node_zone: "none".into(),
            module_risk: "none".into(),
            input_risk: "none".into(),
            output_risk: "none".into(),
            valid: true,
            reasons: vec![],
        };

        if step.func.is_empty() {
            return Err("Device, module, or function missing in the step.".into());
        }

        let nodecard = find_one::<NodeCard>("nodecards", doc! { "nodeid": &device_hex })
            .await
            .map_err(|e| format!("nodecards.findOne error: {e}"))?;

        if nodecard.is_none() {
            log.valid = false;
            log.reasons
                .push(format!("Node card not found for device {device_hex}"));
            logs.push(log);
            continue;
        }
        let nodecard = nodecard.unwrap();
        log.node_zone = nodecard.zone.clone();

        let modulecard = find_one::<ModuleCard>("modulecards", doc! { "moduleid": &step.module.to_hex() })
            .await
            .map_err(|e| format!("modulecards.findOne error: {e}"))?;

        if modulecard.is_none() {
            log.valid = false;
            log.reasons
                .push(format!("Module card not found for module {module_hex}"));
            logs.push(log);
            continue;
        }
        let modulecard = modulecard.unwrap();
        let risk_level_module = if let Some(r) = modulecard.risk_level {
            Ok(r)
        } else {
            Err("Module card was missing risk level, failed to validate".to_string())
        }?;
        log.module_risk = risk_level_module.clone();

        let allowed = zone_allowed
            .get(&nodecard.zone)
            .cloned()
            .unwrap_or_default();
        if !allowed.iter().any(|x| x == &risk_level_module) {
            log.valid = false;
            log.reasons.push(format!(
                "Module risk level '{}' not allowed in zone '{}'",
                risk_level_module, nodecard.zone
            ));
        } else {
            log.reasons.push(format!(
                "Module risk level '{}' allowed in zone '{}'",
                risk_level_module, nodecard.zone
            ));
        }

        let mut datasource_risk: Option<String> = None;
        let input_type_module = if let Some(i) = modulecard.input_type {
            Ok(i)
        } else {
            Err("Module card didnt have an input type, deployment failed to validate")
        }?;
        if input_type_module != "temp" {
            let ds = find_one::<DatasourceCard>(
                "datasourcecards",
                doc! { "type": &input_type_module, "nodeid": &step.device },
            )
            .await
            .map_err(|e| format!("datasourcecards.findOne error: {e}"))?;

            if let Some(ds_card) = ds {
                log.input_risk = ds_card.risk_level.clone();
                datasource_risk = Some(ds_card.risk_level.clone());
                log.reasons.push(format!(
                    "Data source risk level '{}' found for input type '{}'",
                    log.input_risk, input_type_module
                ));
            } else {
                log.valid = false;
                log.reasons.push(format!(
                    "Data source card not found for input type '{}' on device {}",
                    input_type_module, device_hex
                ));
            }
        } else {
            log.input_risk = output_risk.clone();
            log.reasons.push(format!(
                "Input type is temporary, inheriting risk level '{}'",
                log.input_risk
            ));
        }

        if !allowed.iter().any(|x| x == &log.input_risk) {
            log.valid = false;
            log.reasons.push(format!(
                "Input risk level '{}' not allowed in zone '{}'",
                log.input_risk, nodecard.zone
            ));
        } else {
            log.reasons.push(format!(
                "Input risk level '{}' allowed in zone '{}'",
                log.input_risk, nodecard.zone
            ));
        }

        let output_risk_module_card = if let Some(o) = modulecard.output_risk {
            Ok(o)
        } else {
            Err("Module card was missing its output risk, failed to validate deployment")
        }?;
        if output_risk_module_card == "inherit" {
            if let Some(ds_risk) = datasource_risk {
                output_risk = ds_risk;
            }
            log.reasons
                .push(format!("Module output risk level inherited as '{}'", output_risk));
        } else {
            output_risk = output_risk_module_card.clone();
            log.reasons
                .push(format!("Module output risk level set to '{}'", output_risk));
        }
        log.output_risk = output_risk.clone();

        if !allowed.iter().any(|x| x == &output_risk) {
            log.valid = false;
            log.reasons.push(format!(
                "Output risk level '{}' not allowed in zone '{}'",
                output_risk, nodecard.zone
            ));
        } else {
            log.reasons.push(format!(
                "Output risk level '{}' allowed in zone '{}'",
                output_risk, nodecard.zone
            ));
        }

        if log.valid {
            log.reasons.push("Step validated successfully.".into());
        }

        logs.push(log);
    }

    let all_valid = logs.iter().all(|l| l.valid);
    let cert = DeploymentCertificate {
        id: None,
        date: Utc::now(),
        deployment_id: deployment_id.clone(),
        valid: all_valid,
        validation_logs: logs,
    };

    insert_one("deploymentcertificates", &cert)
        .await
        .map_err(|e| format!("insert certificate failed: {e}"))?;

    if !all_valid {
        return Err("Deployment validation failed.".into());
    }
    Ok(())
}


/// Endpoint for getting all deployment certificates
pub async fn get_deployment_certificates() -> Result<impl Responder, ApiError> {
    let coll = get_collection::<bson::Document>("deploymentcertificates").await;

    let mut cursor = coll.find(doc! {}).await.map_err(ApiError::db)?;
    let mut out: Vec<bson::Document> = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(ApiError::db)? {
        out.push(doc);
    }
    let mut v = serde_json::to_value(&out).map_err(ApiError::db)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}
