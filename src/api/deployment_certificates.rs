use chrono::Utc;
use std::collections::HashMap;
use futures::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use actix_web::{HttpResponse, Responder};
use crate::lib::mongodb::{get_collection, find_one, insert_one};
use crate::api::deployment::CreateSolutionResult;
use crate::structs::deployment_certificates::{DeploymentCertificate, ValidationLog};
use crate::structs::node_cards::NodeCard;
use crate::structs::data_source_cards::DatasourceCard;
use crate::structs::zones::Zones;
use crate::structs::module_cards::ModuleCard;
use crate::lib::errors::ApiError;
use crate::lib::constants::{
    COLL_ZONES,
    COLL_MODULE_CARDS,
    COLL_NODE_CARDS,
    COLL_DATASOURCE_CARDS,
    COLL_DEPLOYMENT_CERTS,
};


/// Validates that a given deployment fulfills all constraints (zones, node cards, module cards, data source cards).
pub async fn validate_deployment_solution(
    deployment_id: &ObjectId,
    solution: &CreateSolutionResult,
) -> Result<(), String> {

    // Build a map: zone_name -> allowed risk levels
    let zones_coll = get_collection::<Zones>(COLL_ZONES).await;
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

    // Validate each step in the deployment separately
    for step in &solution.sequence {
        let device_hex = step.device.to_hex();
        let module_hex = step.module.to_hex();

        // Create log to store the validation results and reasoning for this step
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

        // Load module card and node card, and check that they exist and have valid format
        let nodecard = find_one::<NodeCard>(COLL_NODE_CARDS, doc! { "nodeid": step.device })
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
        let modulecard =
            find_one::<ModuleCard>(COLL_MODULE_CARDS, doc! { "moduleid": step.module })
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
        let risk_level_module = if modulecard.risk_level.is_empty() {
            return Err("Module card was missing risk level, failed to validate".to_string());
        } else {
            modulecard.risk_level.clone()
        };
        log.module_risk = risk_level_module.clone();

         // Check that module has a valid risk level given the zone of the node its deployed to
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

        // Get input risk level
        let mut datasource_risk: Option<String> = None;
        let input_type_module = if modulecard.input_type.is_empty() {
            return Err("Module card didnt have an input type, deployment failed to validate".to_string());
        } else {
            modulecard.input_type.clone()
        };
        if input_type_module != "temp" {
            let ds = find_one::<DatasourceCard>(
                COLL_DATASOURCE_CARDS,
                doc! { "type": &input_type_module, "nodeid": step.device },
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

        // Check input risk against zone
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

        // Get output risk level
        let output_risk_module_card = &modulecard.output_risk;
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

        // Check output risk against zone
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

    // If any step was invalid, the whole deployment is invalid
    let all_valid = logs.iter().all(|l| l.valid);
    let cert = DeploymentCertificate {
        id: None,
        date: Utc::now(),
        deployment_id: deployment_id.clone(),
        valid: all_valid,
        validation_logs: logs,
    };
    insert_one(COLL_DEPLOYMENT_CERTS, &cert)
        .await
        .map_err(|e| format!("insert certificate failed: {e}"))?;
    if !all_valid {
        return Err("Deployment validation failed.".into());
    }
    Ok(())
}


/// GET /deploymentCertificates
/// Returns all deployment certificates.
pub async fn get_deployment_certificates() -> Result<impl Responder, ApiError> {
    let coll = get_collection::<DeploymentCertificate>(COLL_DEPLOYMENT_CERTS).await;
    let mut cursor = coll.find(doc! {}).await.map_err(ApiError::db)?;
    let mut out: Vec<DeploymentCertificate> = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(ApiError::db)? {
        out.push(doc);
    }

    // Normalize object ids before returning (UI compatibility)
    let mut v = serde_json::to_value(&out).map_err(ApiError::db)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}
