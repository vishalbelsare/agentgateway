use serde::{Deserialize, Serialize};
use std::default::Default;

pub(crate) mod gcp;

#[async_trait::async_trait]
pub trait BackendAuth: Send + Sync {
	async fn get_token(&self) -> Result<String, anyhow::Error>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(tag = "type")]
pub enum BackendAuthConfig {
	#[default]
	#[serde(rename = "gcp")]
	GCP,
}

pub async fn build(auth_impl: BackendAuthConfig) -> impl BackendAuth {
	match auth_impl {
		BackendAuthConfig::GCP => gcp::GCPBackend::new().await.unwrap(),
	}
}
