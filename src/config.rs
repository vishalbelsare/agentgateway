use std::collections::HashMap;
pub struct Config {
	pub xds_address: Option<String>,
	pub alt_xds_hostname: Option<String>,
	pub metadata: HashMap<String, String>,
}
