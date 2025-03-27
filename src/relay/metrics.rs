use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

use crate::metrics::Recorder;

pub struct Metrics {
	tool_calls: Family<ToolCall, Counter>,
	tool_call_errors: Family<ToolCallError, Counter>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ToolCall {
	pub server: String,
	pub name: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ToolCallError {
	pub server: String,
	pub name: String,
	pub error_type: String,
}

impl Metrics {
	pub fn new(registry: &mut Registry) -> Self {
		let tool_calls = Family::default();
		registry.register(
			"tool_calls",
			"The total number of tool calls",
			tool_calls.clone(),
		);

		let tool_call_errors = Family::default();
		registry.register(
			"tool_call_errors",
			"The total number of tool call errors",
			tool_call_errors.clone(),
		);
		Self {
			tool_calls,
			tool_call_errors,
		}
	}
}

impl Recorder<ToolCall, ()> for Metrics {
	fn record(&self, tool_call: &ToolCall, _: ()) {
		self.tool_calls.get_or_create(tool_call).inc();
	}
}

impl Recorder<ToolCallError, ()> for Metrics {
	fn record(&self, tool_call_error: &ToolCallError, _: ()) {
		self.tool_call_errors.get_or_create(tool_call_error).inc();
	}
}
