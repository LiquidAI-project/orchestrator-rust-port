pub mod api {
    pub mod core_services;
    pub mod data_source_cards;
    pub mod datalist;
    pub mod deployment_certificates;
    pub mod deployment;
    pub mod device;
    pub mod execution;
    pub mod index;
    pub mod logs;
    pub mod module_cards;
    pub mod module;
    pub mod node_cards;
    pub mod zones_and_risk_levels;
}

pub mod lib {
    pub mod constants;
    pub mod dependency_tree;
    pub mod servapp;
    pub mod mongodb;
    pub mod zeroconf;
}