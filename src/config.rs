use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
	pub xds_address: Option<String>,
	pub alt_xds_hostname: Option<String>,
	pub metadata: HashMap<String, String>,
}
