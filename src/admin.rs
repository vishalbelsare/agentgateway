// Copyright Istio Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{mem, sync::Arc};

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
