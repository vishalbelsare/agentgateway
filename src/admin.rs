use std::sync::Arc;

use crate::xds::XdsStore;
use axum::{Router, extract::State, http::StatusCode, routing::get};

#[derive(Clone)]
pub struct App {
	state: Arc<std::sync::RwLock<XdsStore>>,
}

impl App {
	pub fn new(state: Arc<std::sync::RwLock<XdsStore>>) -> Self {
		Self { state }
	}
	pub fn router(&self) -> Router {
		Router::new()
			.route("/targets", get(targets_handler))
			.route("/rbac", get(rbac_handler))
			.route("/listeners", get(listener_handler))
			.with_state(self.clone())
	}
}

async fn targets_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let targets = app.state.read().unwrap().targets.clone();
	let json_targets = serde_json::to_string(&targets).unwrap();
	Ok(json_targets)
}

async fn rbac_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let rbac = app.state.read().unwrap().policies.clone();
	let json_rbac = serde_json::to_string(&rbac).unwrap();
	Ok(json_rbac)
}

async fn listener_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let listener = app.state.read().unwrap().listener.clone();
	let json_listener = serde_json::to_string(&listener).unwrap();
	Ok(json_listener)
}
