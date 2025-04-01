use aws_config::{BehaviorVersion, SdkConfig, meta::region::RegionProviderChain};
use aws_sdk_lambda::{Client, Error, config::Region, meta::PKG_VERSION};
use bytes::Bytes;
use futures::{FutureExt, Sink, Stream, StreamExt, future::BoxFuture, stream::BoxStream};
use sse_stream::{Error as SseError, Sse, SseStream};

use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::sse::{SseClient, SseTransportError};

#[derive(Debug, Clone)]
pub struct Opt {
	/// The AWS Region.
	pub region: Option<String>,

	/// Whether to display additional runtime information.
	pub verbose: bool,
}

#[derive(Debug, Clone)]
pub struct ArnOpt {
	// #[structopt(flatten)]
	pub base: Opt,

	/// The AWS Lambda function's Amazon Resource Name (ARN).
	// #[structopt(short, long)]
	pub arn: String,
}

pub fn make_region_provider(opt: Option<String>) -> RegionProviderChain {
	RegionProviderChain::first_try(opt.map(Region::new))
		.or_default_provider()
		.or_else(Region::new("us-west-2"))
}

pub async fn make_config(opt: Opt) -> SdkConfig {
	let region_provider = make_region_provider(opt.region);

	if opt.verbose {
		tracing::info!("Lambda client version: {}", PKG_VERSION);
		tracing::info!(
			"Region:                {}",
			region_provider.region().await.unwrap().as_ref()
		);
	}

	aws_config::defaults(BehaviorVersion::v2025_01_17())
		.region(region_provider)
		.load()
		.await
}

#[derive(Debug, Clone)]
pub enum AWSBackendError {
	Transport,
}

impl std::fmt::Display for AWSBackendError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self)
	}
}

impl std::error::Error for AWSBackendError {}

#[derive(Clone, Debug)]
pub struct AWSBackend {
	client: aws_sdk_lambda::Client,
	arn: String,
}

impl AWSBackend {
	pub async fn new(opt: ArnOpt) -> Result<Self, anyhow::Error> {
		let config = make_config(opt.base).await;
		let client = aws_sdk_lambda::Client::new(&config);
		Ok(Self {
			client,
			arn: opt.arn,
		})
	}
}

impl From<AWSBackendError> for SseTransportError<AWSBackendError> {
	fn from(value: AWSBackendError) -> Self {
		SseTransportError::Transport(value)
	}
}

impl SseClient<AWSBackendError> for AWSBackend {
	fn connect(
		&self,
		last_event_id: Option<String>,
	) -> BoxFuture<
		'static,
		Result<BoxStream<'static, Result<Sse, SseError>>, SseTransportError<AWSBackendError>>,
	> {
		let client = self.client.clone();
		let arn = self.arn.clone();
		let fut = async move {
			let mut resp = client
				.invoke_with_response_stream()
				.function_name(arn.as_str())
				.send()
				.await
				.map_err(|e| match e {
					aws_smithy_runtime_api::client::result::SdkError::ConstructionFailure(e) => {
						AWSBackendError::Transport
					},
					aws_smithy_runtime_api::client::result::SdkError::TimeoutError(e) => {
						AWSBackendError::Transport.into()
					},
					aws_smithy_runtime_api::client::result::SdkError::DispatchFailure(e) => {
						AWSBackendError::Transport.into()
					},
					aws_smithy_runtime_api::client::result::SdkError::ResponseError(e) => {
						AWSBackendError::Transport.into()
					},
					aws_smithy_runtime_api::client::result::SdkError::ServiceError(e) => {
						AWSBackendError::Transport.into()
					},
					_ => AWSBackendError::Transport.into(),
				})?;

			let event_stream = resp.event_stream;
			let byte_stream = create_stream(event_stream);

			let stream = SseStream::from_byte_stream(byte_stream).boxed();
			Ok(stream)
		};
		fut.boxed()
	}

	fn post(
		&self,
		endpoint: &str,
		message: ClientJsonRpcMessage,
	) -> BoxFuture<'static, Result<(), SseTransportError<AWSBackendError>>> {
		todo!()
	}
}

fn create_stream(
	stream: aws_sdk_lambda::primitives::event_stream::EventReceiver<
		aws_sdk_lambda::types::InvokeWithResponseStreamResponseEvent,
		aws_sdk_lambda::types::error::InvokeWithResponseStreamResponseEventError,
	>,
) -> impl futures::Stream<Item = Result<Bytes, SseError>> {
	futures::stream::unfold(stream, |mut stream| async move {
		let event = stream.recv().await;
		let event = event.unwrap();
		match event {
			Some(event) => {
				match event {
					aws_sdk_lambda::types::InvokeWithResponseStreamResponseEvent::InvokeComplete(_) => {
						// Stream is complete, return empty bytes
						None
					},
					aws_sdk_lambda::types::InvokeWithResponseStreamResponseEvent::PayloadChunk(event) => {
						// Stream is a payload chunk, return the payload
						match event.payload {
							Some(payload) => {
								let bytes = Bytes::from(payload.into_inner());
								Some((Ok(bytes), stream))
							},
							None => None,
						}
					},
					_ => panic!("Unknown event: {:?}", event),
				}
			},
			None => None,
		}
	})
}
