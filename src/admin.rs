use std::sync::Arc;

use crate::xds::XdsStore;
use axum::{Router, extract::State, http::StatusCode, routing::get};
use tracing::error;
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
	match serde_json::to_string(&targets) {
		Ok(json_targets) => Ok(json_targets),
		Err(e) => {
			error!("error serializing targets: {:?}", e);
			Err(StatusCode::INTERNAL_SERVER_ERROR)
		},
	}
}

async fn rbac_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let rbac = app.state.read().unwrap().policies.clone();
	match serde_json::to_string(&rbac) {
		Ok(json_rbac) => Ok(json_rbac),
		Err(e) => {
			error!("error serializing rbac: {:?}", e);
			Err(StatusCode::INTERNAL_SERVER_ERROR)
		},
	}
}

async fn listener_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let listener = app.state.read().unwrap().listener.clone();
	match serde_json::to_string(&listener) {
		Ok(json_listener) => Ok(json_listener),
		Err(e) => {
			error!("error serializing listener: {:?}", e);
			Err(StatusCode::INTERNAL_SERVER_ERROR)
		},
	}
}
