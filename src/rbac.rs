use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use http::header::{AUTHORIZATION, HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::map::Map;

#[derive(Clone)]
pub struct RbacEngine {
	rules: Vec<Rule>,
	claims: Claims,
}

impl RbacEngine {
	pub fn new(rules: Vec<Rule>, claims: Claims) -> Self {
		Self { rules, claims }
	}

	pub fn passthrough() -> Self {
		Self {
			rules: vec![],
			claims: Claims { claims: Map::new() },
		}
	}
	// Check if the claims have access to the resource
	pub fn check(&self, resource: ResourceType) -> bool {
		tracing::info!("Checking RBAC for resource: {:?}", resource);
		// If there are no rules, everyone has access
		if self.rules.is_empty() {
			return true;
		}

		self.rules.iter().any(|rule| {
			rule.resource.matches(&resource) && self.claims.matches(&rule.key, &rule.value, &rule.matcher)
		})
	}
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
	key: String,
	value: String,
	matcher: Matcher,
	resource: ResourceType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum ResourceType {
	Tool { id: String },
	Prompt { id: String },
	Resource { id: String },
}

impl ResourceType {
	pub fn matches(&self, other: &Self) -> bool {
		// Support wildcard
		match (self, other) {
			(ResourceType::Tool { id: a }, ResourceType::Tool { id: b }) => a == b || a == "*",
			(ResourceType::Prompt { id: a }, ResourceType::Prompt { id: b }) => a == b || a == "*",
			(ResourceType::Resource { id: a }, ResourceType::Resource { id: b }) => a == b || a == "*",
			_ => false,
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum Matcher {
	Equals,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
	claims: Map<String, Value>,
}

impl Claims {
	pub fn new(headers: &HeaderMap) -> Self {
		match get_claims(headers) {
			Some(claims) => Self {
				claims: claims.claims,
			},
			None => Self { claims: Map::new() },
		}
	}

	pub fn matches(&self, key: &str, value: &str, matcher: &Matcher) -> bool {
		match matcher {
			Matcher::Equals => self.get_claim(key) == Some(value),
		}
	}
	fn get_claim(&self, key: &str) -> Option<&str> {
		self.claims.get(key).and_then(|v| v.as_str())
	}
}
// TODO: Swap to error
fn get_claims(headers: &HeaderMap) -> Option<Claims> {
	let auth_header = headers.get(AUTHORIZATION);
	match auth_header {
		Some(auth_header) => {
			// TODO: Handle errors
			// Should never happen because this means it's non-ascii
			let auth_header_value = auth_header.to_str().unwrap();
			let parts: Vec<&str> = auth_header_value.splitn(2, " ").collect();
			if parts.len() != 2 || parts[0] != "Bearer" {
				return None;
			}
			let token = parts[1];
			let claims = decode_jwt(token);
			claims
		},
		None => return None,
	}
}

fn decode_jwt(token: &str) -> Option<Claims> {
	//
	/*
		Split the token into header, payload, and signature.
		The parts are separated by a dot (.).

		For example:

		eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.KMUFsIDTnFmyG3nMiGM6H9FNFUROf3wh7SmqJp-QV30

		{"alg":"HS256","typ":"JWT"}{"sub":"1234567890","name":"John Doe","admin":true,"iat":1516239022}<secret_data>
	*/
	let parts: Vec<&str> = token.splitn(3, ".").collect();
	if parts.len() < 2 {
		return None;
	}

	let payload = parts[1];
	match STANDARD_NO_PAD.decode(payload) {
		Ok(decoded) => match serde_json::from_slice(&decoded) {
			Ok(claims) => Some(Claims { claims }),
			Err(e) => {
				tracing::info!("Error parsing JWT payload: {}", e);
				None
			},
		},
		Err(e) => {
			println!("Error decoding JWT: {}", e);
			None
		},
	}
}

#[test]
fn test_decode_jwt() {
	let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.KMUFsIDTnFmyG3nMiGM6H9FNFUROf3wh7SmqJp-QV30";
	let claims = decode_jwt(token);
	assert!(claims.is_some());
	let claims = claims.unwrap();
	assert_eq!(
		claims.claims.get("sub"),
		Some(&Value::String("1234567890".to_string()))
	);
	assert_eq!(
		claims.claims.get("name"),
		Some(&Value::String("John Doe".to_string()))
	);
	assert_eq!(claims.claims.get("admin"), Some(&Value::Bool(true)));
	assert_eq!(
		claims.claims.get("iat"),
		Some(&Value::Number(serde_json::Number::from(1516239022)))
	);
}

#[test]
fn test_get_claims() {
	let headers = HeaderMap::new();
	let claims = get_claims(&headers);
	assert!(claims.is_none());
}

#[test]
fn test_resource_matches() {
	let resource1 = ResourceType::Tool {
		id: "increment".to_string(),
	};
	let resource2 = ResourceType::Tool {
		id: "*".to_string(),
	};
	assert!(resource2.matches(&resource1));

	let resource1 = ResourceType::Prompt {
		id: "increment".to_string(),
	};
	let resource2 = ResourceType::Prompt {
		id: "increment".to_string(),
	};
	assert!(resource2.matches(&resource1));

	let resource1 = ResourceType::Resource {
		id: "increment".to_string(),
	};
	let resource2 = ResourceType::Resource {
		id: "increment_2".to_string(),
	};
	assert!(!resource2.matches(&resource1));

	let resource1 = ResourceType::Resource {
		id: "*".to_string(),
	};
	let resource2 = ResourceType::Resource {
		id: "increment".to_string(),
	};
	assert!(!resource2.matches(&resource1));
}

#[test]
fn test_rbac_false_check() {
	let rules = vec![Rule {
		key: "user".to_string(),
		value: "admin".to_string(),
		matcher: Matcher::Equals,
		resource: ResourceType::Tool {
			id: "increment".to_string(),
		},
	}];
	let mut headers = HeaderMap::new();
	headers.insert(AUTHORIZATION, http::header::HeaderValue::from_str("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.KMUFsIDTnFmyG3nMiGM6H9FNFUROf3wh7SmqJp-QV30").unwrap());
	let rbac = RbacEngine::new(rules, Claims::new(&headers));
	assert!(!rbac.check(ResourceType::Tool {
		id: "increment".to_string()
	}));
}

#[test]
fn test_rbac_check() {
	let rules = vec![Rule {
		key: "sub".to_string(),
		value: "1234567890".to_string(),
		matcher: Matcher::Equals,
		resource: ResourceType::Tool {
			id: "increment".to_string(),
		},
	}];
	let mut headers = HeaderMap::new();
	headers.insert(AUTHORIZATION, http::header::HeaderValue::from_str("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.KMUFsIDTnFmyG3nMiGM6H9FNFUROf3wh7SmqJp-QV30").unwrap());
	let rbac = RbacEngine::new(rules, Claims::new(&headers));
	assert!(rbac.check(ResourceType::Tool {
		id: "increment".to_string()
	}));
}
