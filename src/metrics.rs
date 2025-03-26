use std::{mem, sync::Arc};

use axum::{Router, extract::State, http::StatusCode, routing::get};

use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use tracing::error;

/// Creates a metrics sub registry for mcpproxy.
pub fn sub_registry(registry: &mut Registry) -> &mut Registry {
	registry.sub_registry_with_prefix("mcpproxy")
}

pub struct Deferred<'a, F, T>
where
	F: FnOnce(&'a T),
	T: ?Sized,
{
	param: &'a T,
	deferred_fn: Option<F>,
}

impl<'a, F, T> Deferred<'a, F, T>
where
	F: FnOnce(&'a T),
	T: ?Sized,
{
	pub fn new(param: &'a T, deferred_fn: F) -> Self {
		Self {
			param,
			deferred_fn: Some(deferred_fn),
		}
	}
}

impl<'a, F, T> Drop for Deferred<'a, F, T>
where
	F: FnOnce(&'a T),
	T: ?Sized,
{
	fn drop(&mut self) {
		if let Some(deferred_fn) = mem::take(&mut self.deferred_fn) {
			(deferred_fn)(self.param);
		} else {
			error!("defer deferred record failed, event is gone");
		}
	}
}

pub trait DeferRecorder {
	#[must_use = "metric will be dropped (and thus recorded) immediately if not assigned"]
	/// Perform a record operation on this object when the returned [Deferred] object is
	/// dropped.
	fn defer_record<'a, F>(&'a self, record: F) -> Deferred<'a, F, Self>
	where
		F: FnOnce(&'a Self),
	{
		Deferred::new(self, record)
	}
}

pub trait Recorder<E, T> {
	/// Record the given event
	fn record(&self, event: &E, meta: T);
}

pub trait IncrementRecorder<E>: Recorder<E, u64> {
	/// Record the given event by incrementing the counter by count
	fn increment(&self, event: &E);
}

impl<E, R> IncrementRecorder<E> for R
where
	R: Recorder<E, u64>,
{
	fn increment(&self, event: &E) {
		self.record(event, 1);
	}
}

#[derive(Clone, Default)]
pub struct App {
	registry: Arc<Registry>,
}

impl App {
	pub fn new(registry: Arc<Registry>) -> Self {
		Self { registry }
	}
	pub fn router(&self) -> Router {
		Router::new()
			.route("/metrics", get(metrics_handler))
			.with_state(self.clone())
	}
}

async fn metrics_handler(State(app): State<App>) -> Result<String, StatusCode> {
	let mut buffer = String::new();
	match encode(&mut buffer, &app.registry) {
		Ok(_) => Ok(buffer),
		Err(e) => {
			error!("error encoding metrics: {:?}", e);
			Err(StatusCode::INTERNAL_SERVER_ERROR)
		},
	}
}
