use axum::{Router, extract::State, http::StatusCode, routing::get};
use std::collections::HashMap;
use std::{sync::Arc};

use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::admin::add_cors_layer;

#[derive(Clone, Default)]
struct App {
	registry: Arc<Registry>,
}

impl App {
	fn new(registry: Arc<Registry>) -> Self {
		Self { registry }
	}
	fn router(&self) -> Router {
		Router::new()
			.route("/metrics", get(metrics_handler))
			.layer(add_cors_layer())
			.with_state(self.clone())
	}
}

async fn metrics_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let mut buffer = String::new();
	match encode(&mut buffer, &app.registry) {
		Ok(_) => Ok(buffer),
		Err(e) => {
			error!("error encoding metrics: {:?}", e);
			Err(StatusCode::INTERNAL_SERVER_ERROR)
		},
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
	#[serde(default = "default_host")]
	pub host: String,
	#[serde(default = "default_port")]
	pub port: u16,
	#[serde(default)]
	pub tags: HashMap<String, String>,
}

fn default_host() -> String {
	"0.0.0.0".to_string()
}

fn default_port() -> u16 {
	9091
}

impl Default for Config {
	fn default() -> Self {
		Self {
			host: default_host(),
			port: default_port(),
			tags: HashMap::new(),
		}
	}
}

pub async fn start(
	registry: Arc<Registry>,
	ct: tokio_util::sync::CancellationToken,
	cfg: Option<Config>,
) -> Result<(), std::io::Error> {
	let cfg = cfg.unwrap_or_default();
	let listener = tokio::net::TcpListener::bind(format!("{}:{}", cfg.host, cfg.port)).await?;
	let app = App::new(registry);
	let router = app.router();
	axum::serve(listener, router)
		.with_graceful_shutdown(async move {
			ct.cancelled().await;
		})
		.await
}
