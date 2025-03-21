use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
	pub xds_address: Option<String>,
	pub alt_xds_hostname: Option<String>,
	pub metadata: HashMap<String, String>,
}
