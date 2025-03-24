use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::xds::Listener;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
	pub xds_address: Option<String>,
	pub alt_xds_hostname: Option<String>,
	pub metadata: HashMap<String, String>,
	pub listener: Listener,
}
