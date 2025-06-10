use http_body::Body;
use pin_project_lite::pin_project;
use std::{
	cmp,
	convert::Infallible,
	future::Future,
	pin::Pin,
	task::{Context, Poll, ready},
	time::Duration,
};
use tokio::time::{Instant, Sleep, sleep, sleep_until};

pub struct BodyTransform<B: Body> {
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
impl<B: Body+Default> BodyTransform<B> {

	pub fn apply(&self, r: crate::http::Response) -> crate::http::Response {
		let transform: BodyTransform<B> = self.clone();
		r.map(|b| crate::http::Body::new(TransformBody::new(transform, b)))
	}
}

pin_project! {
	pub struct TransformBody<B: Body> {
		transform: BodyTransform<B>,

		#[pin]
		body: B,

		buffer: <B as Body>::Data,
		next_frame: Option<Option<Result<http_body::Frame<B::Data>, B::Error>>>,
	}
}

impl<B> TransformBody<B>
where
	B: Body,
	B::Data: Default,
	B::Error: Into<axum_core::BoxError>,
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

impl<B> TransformBody<B>
where
	B: Body,
	B::Error: Into<axum_core::BoxError>,
{
	fn transform(
		buffer: Option<<B as Body>::Data>,
		new_data: <B as Body>::Data,
	) -> (Option<<B as Body>::Data>, <B as Body>::Data) {
		todo!("transform");
	}
	fn transform2(new_data: <B as Body>::Data) -> (<B as Body>::Data) {
		todo!("transform");
	}

	fn fix_pii(
		buffer: &mut <B as Body>::Data,
		frame: Option<<B as Body>::Data>,
	) -> Option<<B as Body>::Data> {
		// if we have an error send it through.
		// if we have buffered trailers send them.
		// if we have a buffer inside us, and its trailers or None or end_stream transform and send it. if we have trailers buffer them.
		//
		// now this means we must have a body!
		// otherwise call transform on the (self.buffer, buffer) and return the result

		//	let (data_to_buffer, data_to_send) = Self::transform(buffer, data.into_data().map_err(|e| ()).unwrap());
		//	buffer = data_to_buffer;
		//	Some(http_body::Frame::data(data_to_send))
		frame
	}
}

impl<B> Body for TransformBody<B>
where
	B: Body,
	B::Error: Into<axum_core::BoxError>,
{
	type Data = B::Data;
	type Error = B::Error;
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
		loop {
			let frame = ready!(this.body.as_mut().poll_frame(cx));
			return match frame {
				Some(Ok(data)) if data.is_trailers() => match Self::fix_pii(this.buffer, None) {
					Some(frame) => {
						*this.next_frame = Some(Some(Ok(data)));
						Poll::Ready(Some(Ok(http_body::Frame::data(frame))))
					},
					None => Poll::Ready(Some(Ok(data))),
				},
				Some(Ok(data)) if data.is_data() => {
					match Self::fix_pii(this.buffer, Some(data.into_data().map_err(|e| ()).unwrap())) {
						Some(frame) => Poll::Ready(Some(Ok(http_body::Frame::data(frame)))),
						// we are buffering the body - we don't have what to return, so we need to poll again.
						None => continue,
					}
				},
				// ignore frames that are not body or trailers.
				e @ Some(_) => Poll::Ready(e),
				None => match Self::fix_pii(this.buffer, None) {
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
