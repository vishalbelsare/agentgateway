use crate::stream::Socket;
use futures_util::TryFutureExt;
use rustls::ServerConfig;
use rustls::crypto::CryptoProvider;
use std::sync::Arc;

pub static ALL_TLS_VERSIONS: &[&rustls::SupportedProtocolVersion] =
	&[&rustls::version::TLS12, &rustls::version::TLS13];

pub fn provider() -> Arc<CryptoProvider> {
	Arc::new(CryptoProvider {
		// Limit to only the subset of ciphers that are FIPS compatible
		cipher_suites: vec![
			rustls::crypto::ring::cipher_suite::TLS13_AES_256_GCM_SHA384,
			rustls::crypto::ring::cipher_suite::TLS13_AES_128_GCM_SHA256,
		],
		..rustls::crypto::ring::default_provider()
	})
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("tls handshake error: {0:?}")]
	Handshake(std::io::Error),
	#[error("{0}")]
	Anyhow(#[from] anyhow::Error),
}

pub async fn accept(conn: Socket, cfg: Arc<ServerConfig>) -> Result<Socket, Error> {
	let (ext, inner) = conn.into_parts();
	let tls_cfg = cfg.clone();
	let stream = tokio_rustls::TlsAcceptor::from(tls_cfg)
		.accept(Box::new(inner))
		.map_err(Error::Handshake)
		.await?;
	Ok(Socket::from_tls(ext, stream)?)
}
