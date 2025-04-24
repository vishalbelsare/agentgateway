use google_cloud_auth::{credentials, errors};

#[derive(Clone, Debug)]
pub struct GCPBackend {
	credentials: credentials::Credential,
}

impl GCPBackend {
	pub async fn new() -> Result<Self, errors::CredentialError> {
		let credentials = credentials::create_access_token_credential().await?;
		Ok(Self { credentials })
	}
}

#[async_trait::async_trait]
impl crate::backend::BackendAuth for GCPBackend {
	async fn get_token(&self) -> Result<String, anyhow::Error> {
		Ok(self.credentials.get_token().await?.token)
	}
}
