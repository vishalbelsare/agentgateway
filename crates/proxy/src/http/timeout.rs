use http_body::Body;
use pin_project_lite::pin_project;
use std::{
	cmp,
	future::Future,
	pin::Pin,
	task::{Context, Poll, ready},
	time::Duration,
};
use tokio::time::{Instant, Sleep, sleep, sleep_until};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
	pub request_timeout: Option<Duration>,
	pub backend_request_timeout: Option<Duration>,
}

impl Policy {
	pub fn effective_timeout(&self) -> Option<Duration> {
		match self {
			Policy {
				request_timeout: Some(request_timeout),
				backend_request_timeout: Some(backend_request_timeout),
			} => {
				// We do not distinguish these yet, so just take min
				Some(cmp::min(*request_timeout, *backend_request_timeout))
			},
			Policy {
				request_timeout: Some(request_timeout),
				..
			} => Some(*request_timeout),
			Policy {
				backend_request_timeout: Some(backend_request_timeout),
				..
			} => Some(*backend_request_timeout),
			_ => None,
		}
	}
}

pub enum BodyTimeout {
	Duration(Duration),
	Deadline(Instant),
}

impl BodyTimeout {
	pub fn apply(self, r: crate::http::Response) -> crate::http::Response {
		r.map(|b| crate::http::Body::new(TimeoutBody::new(self, b)))
	}
}

pin_project! {
	pub struct TimeoutBody<B> {
		timeout: BodyTimeout,
		#[pin]
		sleep: Option<Sleep>,
		#[pin]
		body: B,
	}
}

impl<B> TimeoutBody<B> {
	/// Creates a new [`TimeoutBody`].
	pub fn new(timeout: BodyTimeout, body: B) -> Self {
		TimeoutBody {
			timeout,
			sleep: None,
			body,
		}
	}
}

impl<B> Body for TimeoutBody<B>
where
	B: Body,
	B::Error: Into<axum_core::BoxError>,
{
	type Data = B::Data;
	type Error = Box<dyn std::error::Error + Send + Sync>;

	fn poll_frame(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
	) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
		let mut this = self.project();

		// Start the `Sleep` if not active.
		let sleep_pinned = if let Some(some) = this.sleep.as_mut().as_pin_mut() {
			some
		} else {
			match this.timeout {
				BodyTimeout::Duration(d) => {
					this.sleep.set(Some(sleep(*d)));
				},
				BodyTimeout::Deadline(d) => {
					this.sleep.set(Some(sleep_until(*d)));
				},
			}
			this.sleep.as_mut().as_pin_mut().unwrap()
		};

		// Error if the timeout has expired.
		if let Poll::Ready(()) = sleep_pinned.poll(cx) {
			return Poll::Ready(Some(Err(Box::new(TimeoutError(())))));
		}

		// Check for body data.
		let frame = ready!(this.body.poll_frame(cx));
		// A frame is ready. Reset the `Sleep`...
		this.sleep.set(None);

		Poll::Ready(frame.transpose().map_err(Into::into).transpose())
	}
}

/// Error for [`TimeoutBody`].
#[derive(Debug)]
pub struct TimeoutError(());

impl std::error::Error for TimeoutError {}

impl std::fmt::Display for TimeoutError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "data was not received within the designated timeout")
	}
}
#[cfg(test)]
mod tests {
	use super::*;

	use bytes::Bytes;
	use http_body::Frame;
	use http_body_util::BodyExt;
	use pin_project_lite::pin_project;
	use std::{error::Error, fmt::Display};

	#[derive(Debug)]
	struct MockError;

	impl Error for MockError {}

	impl Display for MockError {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			write!(f, "mock error")
		}
	}

	pin_project! {
		struct MockBody {
			#[pin]
			sleep: Sleep
		}
	}

	impl Body for MockBody {
		type Data = Bytes;
		type Error = MockError;

		fn poll_frame(
			self: Pin<&mut Self>,
			cx: &mut Context<'_>,
		) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
			let this = self.project();
			this
				.sleep
				.poll(cx)
				.map(|_| Some(Ok(Frame::data(vec![].into()))))
		}
	}

	#[tokio::test]
	async fn test_body_available_within_timeout() {
		let mock_sleep = Duration::from_secs(1);
		let timeout_sleep = Duration::from_secs(2);

		let mock_body = MockBody {
			sleep: sleep(mock_sleep),
		};
		let timeout_body = TimeoutBody::new(BodyTimeout::Duration(timeout_sleep), mock_body);

		assert!(
			timeout_body
				.boxed()
				.frame()
				.await
				.expect("no frame")
				.is_ok()
		);
	}

	#[tokio::test]
	async fn test_body_unavailable_within_timeout_error() {
		let mock_sleep = Duration::from_secs(2);
		let timeout_sleep = Duration::from_secs(1);

		let mock_body = MockBody {
			sleep: sleep(mock_sleep),
		};
		let timeout_body = TimeoutBody::new(BodyTimeout::Duration(timeout_sleep), mock_body);

		assert!(timeout_body.boxed().frame().await.unwrap().is_err());
	}
}
