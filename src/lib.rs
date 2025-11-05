// Library exports for homemetrics crate
// This allows tests and other crates to use the modules

pub mod attachment_parser;
pub mod config;
pub mod database;
pub mod gmail_client;
pub mod slack_notifier;
pub mod email;

// X-Sense temperature monitoring module
pub mod xsense;

// Blue Riot pool monitoring module
pub mod blueriot;
