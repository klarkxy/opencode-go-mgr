pub mod auth;
pub mod crypto;
pub mod dashboard;
pub mod db;
pub mod gateway;
pub mod models;
pub mod state;

pub type Result<T> = anyhow::Result<T>;
