pub mod api {
    pub mod data_source_cards;
    pub mod deployment_certificates;
    pub mod deployment;
    pub mod device;
    pub mod execution;
    pub mod logs;
    pub mod module_cards;
    pub mod module;
    pub mod node_cards;
    pub mod zones_and_risk_levels;
    pub mod ws_logs;
}

pub mod lib {
    pub mod constants;
    pub mod mongodb;
    pub mod zeroconf;
    pub mod utils;
    pub mod initializer;
    pub mod errors;
}

pub mod structs {
    pub mod data_source_cards;
    pub mod deployment_certificates;
    pub mod deployment;
    pub mod device;
    pub mod module_cards;
    pub mod module;
    pub mod node_cards;
    pub mod openapi;
    pub mod zones;
    pub mod logs;
}