use http::header::HeaderMap;
use serde_json::Value;
use serde_json::map::Map;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;


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
    Self { rules: vec![], claims: Claims{claims: Map::new()} }
  }
  // Check if the claims have access to the resource
  pub fn check(&self, resource: ResourceType) -> bool {
    // If there are no rules, everyone has access
    if self.rules.is_empty() {
      return true;
    }

    self.rules.iter().any(|rule| {
      rule.resource == resource && self.claims.matches(&rule.key, &rule.value, &rule.matcher)
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


#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum ResourceType {
  Tool(String),
  Prompt(String),
  Resource(String),
}

impl PartialEq for ResourceType {
  fn eq(&self, other: &Self) -> bool {
    // Support wildcard
    match (self, other) {
      (ResourceType::Tool(a), ResourceType::Tool(b)) => a == b || a == "*",
      (ResourceType::Prompt(a), ResourceType::Prompt(b)) => a == b || a == "*",
      (ResourceType::Resource(a), ResourceType::Resource(b)) => a == b || a == "*",
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
  pub fn new(headers: &HeaderMap) -> Option<Self> {
    match get_claims(headers) {
      Some(claims) => Some(Self { claims: claims.claims }),
      None => None,
    }
  }

  pub fn matches(&self, key: &str, value: &str, matcher: &Matcher) -> bool {
    match matcher {
      Matcher::Equals => {
        self.get_claim(key) == Some(value)
      }
    }
  }
  fn get_claim(&self, key: &str) -> Option<&str> {
    self.claims.get(key).and_then(|v| v.as_str())
  }
}
// TODO: Swap to error
fn get_claims(headers: &HeaderMap) -> Option<Claims> {
  let auth_header = headers.get("Authorization");
  match auth_header {
    Some(auth_header) => {
      let auth_header_value = auth_header.to_str().unwrap();
      let parts: Vec<&str> = auth_header_value.splitn(2, " ").collect();
      if parts.len() != 2 || parts[0] != "Bearer" {
        return None;
      }
      let token = parts[1];
      let claims = decode_jwt(token);
      claims
    }
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
    Ok(decoded) => {
      let claims: Map<String, Value> = serde_json::from_slice(&decoded).unwrap();
      Some(Claims { claims })
    }
    Err(e) => {
      println!("Error decoding JWT: {}", e);
      None
    }
  }
}

#[test]
fn test_decode_jwt() {
  let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.KMUFsIDTnFmyG3nMiGM6H9FNFUROf3wh7SmqJp-QV30";
  let claims = decode_jwt(token);
  assert!(claims.is_some());
  let claims = claims.unwrap();
  assert_eq!(claims.claims.get("sub"), Some(&Value::String("1234567890".to_string())));
  assert_eq!(claims.claims.get("name"), Some(&Value::String("John Doe".to_string())));
  assert_eq!(claims.claims.get("admin"), Some(&Value::Bool(true)));
  assert_eq!(claims.claims.get("iat"), Some(&Value::Number(serde_json::Number::from(1516239022))));
}

#[test]
fn test_get_claims() {
  let headers = HeaderMap::new();
  let claims = get_claims(&headers);
  assert!(claims.is_none());
}
