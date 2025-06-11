use http_body::Body;
use pin_project_lite::pin_project;
use std::{
	pin::Pin,
	task::{Context, Poll, ready},
};

// Policy config - build this once.
pub struct BodyTransform<B: Body> {
	// first arg is the existing buffer, second arg is the new data.
	transform: std::sync::Arc<
		dyn Fn(&mut <B as Body>::Data, Option<<B as Body>::Data>) -> Option<<B as Body>::Data>
			+ Send
			+ Sync,
	>,
}

impl<B: Body> BodyTransform<B> {
	fn new<F>(transform: F) -> Self
	where
		F: Fn(&mut <B as Body>::Data, Option<<B as Body>::Data>) -> Option<<B as Body>::Data>
			+ Send
			+ Sync
			+ 'static,
	{
		Self {
			transform: std::sync::Arc::new(transform),
		}
	}

	fn clone(&self) -> Self {
		Self {
			transform: self.transform.clone(),
		}
	}
}

impl BodyTransform<axum_core::body::Body> {
	pub fn apply(&self, r: crate::http::Response) -> crate::http::Response {
		let transform= self.clone();
		r.map(|b| crate::http::Body::new(TransformBody::new(transform, b)))
	}
}
// Per request state - create this for each request.
pin_project! {
	pub struct TransformBody<B: Body> {
		// the transform to apply to the body.
		transform: BodyTransform<B>,

		// the body stream.
		#[pin]
		body: B,

		// the buffer of the body. maybe used by the transform function.
		// we keep it here so the transform function is stateless.
		buffer: <B as Body>::Data,

		// we may buffer the next frame, if we inject a body.
		next_frame: Option<Option<Result<http_body::Frame<B::Data>, B::Error>>>,
	}
}

impl<B> TransformBody<B>
where
	B: Body,
	B::Data: Default,
{
	/// Creates a new [`TransformBody`].
	pub fn new(transform: BodyTransform<B>, body: B) -> Self {
		TransformBody {
			transform,
			body,
			buffer: <B as Body>::Data::default(),
			next_frame: None,
		}
	}
}

impl<B> Body for TransformBody<B>
where
	B: Body,
{
	type Data = <B as Body>::Data;
	type Error = <B as Body>::Error;
	// type Error = Box<dyn std::error::Error + Send + Sync>;

	fn poll_frame(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
	) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
		let mut this = self.project();

		// if we buffered the trailers, it means we injected a body. so no need to poll, just send them.
		if let Some(next_frame) = this.next_frame.take() {
			return Poll::Ready(next_frame);
		}
		let transform = this.transform.transform.as_ref();
		loop {
			let frame = ready!(this.body.as_mut().poll_frame(cx));
			return match frame {
				Some(Ok(data)) if data.is_trailers() => match transform(this.buffer, None) {
					Some(frame) => {
						*this.next_frame = Some(Some(Ok(data)));
						Poll::Ready(Some(Ok(http_body::Frame::data(frame))))
					},
					None => Poll::Ready(Some(Ok(data))),
				},
				Some(Ok(data)) if data.is_data() => {
					match transform(this.buffer, Some(data.into_data().map_err(|e| ()).unwrap())) {
						Some(frame) => Poll::Ready(Some(Ok(http_body::Frame::data(frame)))),
						// we are buffering the body - we don't have what to return, so we need to poll again.
						None => continue,
					}
				},
				// ignore frames that are not body or trailers.
				e @ Some(_) => Poll::Ready(e),
				None => match transform(this.buffer, None) {
					Some(frame) => {
						*this.next_frame = Some(None);
						Poll::Ready(Some(Ok(http_body::Frame::data(frame))))
					},
					None => Poll::Ready(None),
				},
			};
		}
	}
}
