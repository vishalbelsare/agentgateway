use crate::state::State as AppState;
use anyhow::Result;
use axum::extract::ConnectInfo;
use axum::{
	Json, RequestPartsExt, Router,
	extract::FromRequestParts,
	extract::{Query, State},
	http::{HeaderMap, StatusCode, request::Parts},
	response::sse::{Event, Sse},
	response::{IntoResponse, Response},
	routing::get,
};
use axum_extra::{
	TypedHeader,
	headers::{Authorization, authorization::Bearer},
};
use futures::{SinkExt, StreamExt, stream::Stream};
use jsonwebtoken::{DecodingKey, Validation, decode};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::{ServerHandlerService, serve_server};
use serde_json::Value;
use serde_json::json;
use serde_json::map::Map;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{self};

use crate::relay::Relay;
use crate::{proxyprotocol, rbac};
type SessionId = Arc<str>;

fn session_id() -> SessionId {
	let id = format!("{:016x}", rand::random::<u128>());
	Arc::from(id)
}

#[derive(Clone)]
pub struct App {
	state: Arc<AppState>,
	txs:
		Arc<tokio::sync::RwLock<HashMap<SessionId, tokio::sync::mpsc::Sender<ClientJsonRpcMessage>>>>,
}

impl App {
	pub fn new(state: Arc<AppState>) -> Self {
		Self {
			state,
			txs: Default::default(),
		}
	}
	pub fn router(&self) -> Router {
		Router::new()
			.route("/sse", get(sse_handler).post(post_event_handler))
			.with_state(self.clone())
	}
}

impl FromRequestParts<App> for rbac::Claims {
	type Rejection = AuthError;

	async fn from_request_parts(parts: &mut Parts, _state: &App) -> Result<Self, Self::Rejection> {
		// Extract the token from the authorization header
		let TypedHeader(Authorization(bearer)) = parts
			.extract::<TypedHeader<Authorization<Bearer>>>()
			.await
			.map_err(|_| AuthError::InvalidToken)?;
		// Decode the user data
		let key = DecodingKey::from_secret(b"secret");
		let token_data = decode::<Map<String, Value>>(bearer.token(), &key, &Validation::default())
			.map_err(|_| AuthError::InvalidToken)?;

		Ok(rbac::Claims::new(token_data.claims))
	}
}

impl IntoResponse for AuthError {
	fn into_response(self) -> Response {
		let (status, error_message) = match self {
			AuthError::InvalidToken => (StatusCode::BAD_REQUEST, "Invalid token"),
		};
		let body = Json(json!({
				"error": error_message,
		}));
		(status, body).into_response()
	}
}

#[derive(Debug)]
pub enum AuthError {
	InvalidToken,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostEventQuery {
	pub session_id: String,
}

async fn post_event_handler(
	State(app): State<App>,
	Query(PostEventQuery { session_id }): Query<PostEventQuery>,
	Json(message): Json<ClientJsonRpcMessage>,
) -> Result<StatusCode, StatusCode> {
	tracing::info!(session_id, ?message, "new client message");
	let tx = {
		let rg = app.txs.read().await;
		rg.get(session_id.as_str())
			.ok_or(StatusCode::NOT_FOUND)?
			.clone()
	};
	if tx.send(message).await.is_err() {
		tracing::error!("send message error");
		return Err(StatusCode::GONE);
	}
	Ok(StatusCode::ACCEPTED)
}

async fn sse_handler(
	State(app): State<App>,
	ConnectInfo(connection): ConnectInfo<proxyprotocol::Address>,
	claims: rbac::Claims,
) -> Sse<impl Stream<Item = Result<Event, io::Error>>> {
	// it's 4KB

	let session = session_id();
	tracing::info!(%session, ?connection, "sse connection");
	let claims = rbac::Identity::new(
		Some(claims.claims()),
		match connection.identity {
			Some(identity) => Some(identity),
			None => None,
		},
	);
	use tokio_stream::wrappers::ReceiverStream;
	use tokio_util::sync::PollSender;
	let (from_client_tx, from_client_rx) = tokio::sync::mpsc::channel(64);
	let (to_client_tx, to_client_rx) = tokio::sync::mpsc::channel(64);
	app
		.txs
		.write()
		.await
		.insert(session.clone(), from_client_tx);
	{
		let session = session.clone();
		tokio::spawn(async move {
			let service = ServerHandlerService::new(Relay::new(app.state.clone(), claims));
			let stream = ReceiverStream::new(from_client_rx);
			let sink = PollSender::new(to_client_tx).sink_map_err(std::io::Error::other);
			let result = serve_server(service, (sink, stream))
				.await
				.inspect_err(|e| {
					tracing::error!("serving error: {:?}", e);
				});

			if let Err(e) = result {
				tracing::error!(error = ?e, "initialize error");
				app.txs.write().await.remove(&session);
				return;
			}
			let _running_result = result.unwrap().waiting().await.inspect_err(|e| {
				tracing::error!(error = ?e, "running error");
			});
			app.txs.write().await.remove(&session);
		});
	}

	let stream = futures::stream::once(futures::future::ok(
		Event::default()
			.event("endpoint")
			.data(format!("?sessionId={session}")),
	))
	.chain(ReceiverStream::new(to_client_rx).map(|message| {
		match serde_json::to_string(&message) {
			Ok(bytes) => Ok(Event::default().event("message").data(&bytes)),
			Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
		}
	}));
	Sse::new(stream)
}
