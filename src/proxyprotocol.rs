use axum::extract::connect_info::Connected;
use axum::serve::IncomingStream;
use std::net::SocketAddr;
use tokio::net::TcpListener;

impl Connected<IncomingStream<'_, Listener>> for Address {
	fn connect_info(target: IncomingStream<'_, Listener>) -> Self {
		target.remote_addr().clone()
	}
}

pub struct Listener(TcpListener, bool);

impl Listener {
	pub fn new(s: TcpListener, enabled: bool) -> Self {
		Self(s, enabled)
	}
}

#[derive(Clone, Debug)]
pub struct Address {
	pub addr: SocketAddr,
	pub identity: Option<String>,
}

impl axum::serve::Listener for Listener {
	type Io = <TcpListener as axum::serve::Listener>::Io;
	type Addr = Address;

	async fn accept(&mut self) -> (Self::Io, Self::Addr) {
		let (mut io, addr) = axum::serve::Listener::accept(&mut self.0).await;

		let addr = if !self.1 {
			Address {
				addr,
				identity: None,
			}
		} else {
			let header = protocol::parse(&mut io).await.expect("TODO");
			Address {
				addr,
				identity: header.identity,
			}
		};
		(io, addr)
	}

	fn local_addr(&self) -> std::io::Result<Self::Addr> {
		axum::serve::Listener::local_addr(&self.0).map(|addr| Address {
			addr,
			identity: None,
		})
	}
}

mod protocol {
	use ppp::v2;
	use std::io;
	use std::task::Context;
	use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

	const PROXY_PROTOCOL_AUTHORITY_TLV: u8 = 0xD0;

	#[derive(PartialEq, Debug)]
	pub struct Header {
		pub identity: Option<String>,
	}

	#[derive(thiserror::Error, Debug)]
	pub enum Error {
		#[error("io error: {0}")]
		Io(#[from] io::Error),
		#[error("protocol error")]
		InvalidProtocol,
		#[error("parse error: {0}")]
		Parse(v2::ParseError),
		#[error("imcomplete header (read {0})")]
		Incomplete(usize),
	}

	pub struct PeekReader<'a>(pub &'a mut tokio::net::TcpStream);

	impl AsyncRead for PeekReader<'_> {
		fn poll_read(
			self: std::pin::Pin<&mut Self>,
			cx: &mut Context<'_>,
			buf: &mut ReadBuf<'_>,
		) -> std::task::Poll<std::io::Result<()>> {
			std::pin::Pin::new(&*self.0)
				.poll_peek(cx, buf)
				.map_ok(|_| ())
		}
	}

	pub async fn parse<IO: AsyncRead + Unpin>(source_stream: &mut IO) -> Result<Header, Error> {
		use ppp::PartialResult;
		// Typical header is roughly 50 bytes, but identity could be longer, or they
		// could have more TLVs (why? not sure), so give an ample buffer
		const PEEK_CAPACITY: usize = 512;
		let mut buf = bytes::BytesMut::with_capacity(PEEK_CAPACITY);
		let mut total_read = 0;
		let header = loop {
			let read = source_stream.read_buf(&mut buf).await?;
			if read == 0 {
				return Err(Error::Incomplete(total_read));
			}
			total_read += read;
			// Note: intentionally do not use HeaderResult::parse. Not only is it wasteful to attempt
			// to parse v1, which we will never use, it also has a bug (https://github.com/misalcedo/ppp/issues/28).
			match v2::Header::try_from(buf.as_ref()) {
				Ok(header) => {
					break header;
				},
				Err(e) if !e.is_incomplete() => return Err(Error::Parse(e)),
				_ => {},
			}
			if total_read >= buf.capacity() {
				return Err(Error::Incomplete(total_read));
			}
		};
		let mut identity: Option<String> = None;
		for tlv in header.tlvs() {
			let tlv = tlv.map_err(|_| Error::InvalidProtocol)?;
			tracing::trace!(?tlv, "saw tlv");
			match tlv.kind {
				PROXY_PROTOCOL_AUTHORITY_TLV => {
					let s = std::str::from_utf8(&tlv.value).map_err(|_| Error::InvalidProtocol)?;
					identity = Some(s.to_string());
				},
				_other => {},
			}
		}
		Ok(Header { identity })
	}
}
