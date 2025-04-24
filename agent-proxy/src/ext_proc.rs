use crate::http::{HeaderName, HeaderValue};
use crate::*;
use bytes::Bytes;
use http_body::Frame;
use http_body_util::BodyStream;
use itertools::Itertools;
use std::convert::Infallible;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

#[allow(warnings)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub mod proto {
	tonic::include_proto!("envoy.service.ext_proc.v3");
}

use crate::ext_proc::proto::{
	BodyMutation, BodyResponse, HeadersResponse, HttpBody, HttpHeaders, HttpTrailers, ProcessingRequest,
	ProcessingResponse,
};
use proto::body_mutation::Mutation;
use proto::processing_request::Request;
use proto::processing_response::Response;

pub struct ExtProc {
	tx_req: Sender<ProcessingRequest>,
	rx_resp: Receiver<ProcessingResponse>,
}

impl ExtProc {
	pub async fn new() -> anyhow::Result<ExtProc> {
		error!("howardjohn: connecting...");
		let mut c = proto::external_processor_client::ExternalProcessorClient::connect("http://127.0.0.1:9002").await?;
		error!("howardjohn: connected");
		let (tx_req, rx_req) = tokio::sync::mpsc::channel(10);
		let (tx_resp, rx_resp) = tokio::sync::mpsc::channel(10);
		let req_stream = tokio_stream::wrappers::ReceiverStream::new(rx_req);
		tokio::task::spawn(async move {
			let resp = c.process(req_stream).await.unwrap();
			error!("howardjohn: processed");
			let mut resp = resp.into_inner();
			while let Some(item) = resp.message().await.unwrap() {
				// Process responses here
				error!("howardjohn: received response item");
				tx_resp.send(item).await.unwrap();
			}
		});
		Ok(Self { tx_req, rx_resp })
	}

	async fn send(&mut self, req: ProcessingRequest) -> ProcessingResponse {
		error!("howardjohn: sending req...");
		self.tx_req.send(dbg!(req)).await.expect("TODO");
		error!("howardjohn: sent req...");
		let resp = self.rx_resp.recv().await.expect("TODO");
		error!("howardjohn: got res... {:#?}", resp);
		resp
	}

	async fn recv(&mut self) -> ProcessingResponse {
		let resp = self.rx_resp.recv().await.expect("TODO");
		error!("howardjohn: got res... {:#?}", resp);
		resp
	}

	async fn send2(&mut self, req: ProcessingRequest) {
		error!("howardjohn: sending req...");
		self.tx_req.send(req).await.expect("TODO");
		error!("howardjohn: sent req...");
	}

	pub async fn request_headers(&mut self, req: &mut http::Request) -> http::Request {
		let headers = to_header_map(req.headers());
		let (parts, body) = std::mem::take(req).into_parts();
		// let has_body = !req.body().is_end_stream();
		// IDK why but is_end_stream is always true...
		let has_body = true;
		let preq = processing_request(Request::RequestHeaders(HttpHeaders {
			headers,
			attributes: Default::default(),
			end_of_stream: false,
			// end_of_stream: !has_body,
		}));

		self.send2(preq).await;
		if has_body {
			error!("howardjohn: has body!");
			let tx = self.tx_req.clone();
			tokio::task::spawn(async move {
				let mut stream = BodyStream::new(body);
				while let Some(frame) = stream.next().await {
					let frame = frame.expect("TODO");
					let preq = if frame.is_data() {
						let frame = frame.into_data().expect("already checked");
						processing_request(Request::RequestBody(HttpBody { body: frame.into(), end_of_stream: false }))
					} else if frame.is_trailers() {
						let frame = frame.into_trailers().expect("already checked");
						processing_request(Request::RequestTrailers(HttpTrailers { trailers: to_header_map(&frame) }))
					} else {
						panic!("unknown type")
					};
					error!("howardjohn: sending body req...");
					tx.send(preq).await.unwrap();
				}
				let preq = processing_request(Request::RequestBody(HttpBody {
					body: Default::default(),
					end_of_stream: true,
				}));
				tx.send(preq).await.unwrap();
				error!("howardjohn: body done");
			});
		}
		let (mut tx_chunk, rx_chunk) = tokio::sync::mpsc::channel(1);
		let body = http_body_util::StreamBody::new(ReceiverStream::new(rx_chunk));
		let mut req = http::Request::from_parts(parts, http::Body::new(body));
		loop {
			let resp = self.recv().await;
			if handle_response(&mut req, &mut tx_chunk, resp).await {
				error!("howardjohn: complete!");
				return req;
			}
		}
	}
}

async fn handle_response(
	req: &mut http::Request,
	body_tx: &mut Sender<Result<Frame<Bytes>, Infallible>>,
	presp: ProcessingResponse,
) -> bool {
	let cr = match presp.response {
		Some(Response::RequestHeaders(HeadersResponse { response: Some(cr) })) => cr,
		Some(Response::RequestBody(BodyResponse { response: Some(cr) })) => cr,
		msg => {
			error!("howardjohn: ignoring {msg:?}");
			return false;
		}
	};
	if let Some(h) = cr.header_mutation {
		for rm in &h.remove_headers {
			req.headers_mut().remove(rm);
		}
		for set in h.set_headers {
			let Some(h) = set.header else {
				continue;
			};
			req.headers_mut().insert(
				HeaderName::try_from(h.key).expect("TODO"),
				HeaderValue::from_bytes(h.raw_value.as_slice()).expect("TODO"),
			);
		}
	}
	if let Some(BodyMutation { mutation: Some(b) }) = cr.body_mutation {
		match b {
			Mutation::StreamedResponse(bb) => {
				let eos = bb.end_of_stream;
				let _ = body_tx
					.send(Ok(Frame::data(bytes::Bytes::from(bb.body))))
					.await;
				return eos;
			}
			Mutation::Body(_) => {
				panic!("not valid for streaming mode");
				// *req = std::mem::take(req).map(|_| crate::proxy::http::Body::from(bb));
			}
			Mutation::ClearBody(_) => {
				panic!("not valid for streaming mode");
				// *req = std::mem::take(req).map(|_| crate::proxy::http::Body::empty());
			}
		}
	}
	error!("howardjohn: still waiting for response...");
	false
}

fn to_header_map(headers: &http::HeaderMap) -> Option<proto::HeaderMap> {
	let h = headers
		.iter()
		.map(|(k, v)| proto::HeaderValue { key: k.to_string(), raw_value: v.as_bytes().to_vec() })
		.collect_vec();
	Some(proto::HeaderMap { headers: h })
}

fn processing_request(data: Request) -> ProcessingRequest {
	ProcessingRequest {
		observability_mode: false,
		attributes: Default::default(),
		protocol_config: Default::default(),
		request: Some(data),
	}
}
