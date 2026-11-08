pub mod auth;
pub mod catalog;
pub mod config;
pub mod crypto;
pub mod error;
pub mod http;
pub mod import;
pub mod mcp;
pub mod model;
pub mod sse;
pub mod store;
pub mod tui;
pub mod upstream;
pub mod web;

#[cfg(test)]
mod flow_test;

pub use http::{router, serve, AppState};
