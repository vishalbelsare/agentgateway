use agent_hbone::RWStream;
use hyper_util::client::legacy::connect::{Connected, Connection};
use std::io::{Error, IoSlice};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

#[derive(Debug, Clone)]
pub struct TcpConnectionInfo {
	pub peer_addr: SocketAddr,
	pub local_addr: SocketAddr,
	pub start: Instant,
}

#[derive(Debug, Clone)]
pub struct Identity {}

#[derive(Debug, Clone)]
pub struct TLSConnectionInfo {
	pub src_identity: Option<Identity>,
}

#[derive(Debug, Clone)]
pub struct HBONEConnectionInfo {
	pub hbone_address: SocketAddr,
}

pub struct Socket {
	ext: Extension,
	inner: SocketType,
}

impl Connection for Socket {
	fn connected(&self) -> Connected {
		Connected::new()
	}
}

impl Socket {
	pub fn into_parts(self) -> (Extension, SocketType) {
		(self.ext, self.inner)
	}

	pub fn from_tcp(stream: TcpStream) -> anyhow::Result<Self> {
		let mut ext = Extension::new();
		stream.set_nodelay(true)?;
		ext.insert(TcpConnectionInfo {
			peer_addr: to_canonical(stream.peer_addr()?),
			local_addr: to_canonical(stream.local_addr()?),
			start: Instant::now(),
		});
		Ok(Socket {
			ext,
			inner: SocketType::Tcp(stream),
		})
	}

	pub fn from_tls(mut ext: Extension, tls: TlsStream<Box<SocketType>>) -> anyhow::Result<Self> {
		let src_identity: Option<Identity> = {
			let (_, _ssl) = tls.get_ref();
			// TODO: derive some useful info from the cert
			None
		};
		ext.insert(TLSConnectionInfo { src_identity });
		Ok(Socket {
			ext,
			inner: SocketType::Tls(tls),
		})
	}

	pub fn from_hbone(ext: Arc<Extension>, hbone_address: SocketAddr, hbone: RWStream) -> Self {
		let mut ext = Extension::wrap(ext);
		ext.insert(HBONEConnectionInfo { hbone_address });

		Socket {
			ext,
			inner: SocketType::Hbone(hbone),
		}
	}

	pub fn get_ext(&self) -> Extension {
		self.ext.clone()
	}

	pub fn ext<T: Send + Sync + 'static>(&self) -> Option<&T> {
		self.ext.get::<T>()
	}

	pub fn must_ext<T: Send + Sync + 'static>(&self) -> &T {
		self.ext().expect("expected required extension")
	}

	pub fn tcp(&self) -> &TcpConnectionInfo {
		self.ext.get::<TcpConnectionInfo>().unwrap()
	}
	/// target_address returns the HBONE destination or the L4 destination
	pub fn target_address(&self) -> SocketAddr {
		if let Some(hci) = self.ext.get::<HBONEConnectionInfo>() {
			hci.hbone_address
		} else {
			self.tcp().local_addr
		}
	}
}

pub enum SocketType {
	Tcp(TcpStream),
	Tls(TlsStream<Box<SocketType>>),
	Hbone(RWStream),
	Boxed(Box<SocketType>),
}

impl AsyncRead for SocketType {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut ReadBuf<'_>,
	) -> Poll<std::io::Result<()>> {
		match self.get_mut() {
			SocketType::Tcp(inner) => Pin::new(inner).poll_read(cx, buf),
			SocketType::Tls(inner) => Pin::new(inner).poll_read(cx, buf),
			SocketType::Hbone(inner) => Pin::new(inner).poll_read(cx, buf),
			SocketType::Boxed(inner) => Pin::new(inner).poll_read(cx, buf),
		}
	}
}
impl AsyncWrite for SocketType {
	fn poll_write(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<Result<usize, std::io::Error>> {
		match self.get_mut() {
			SocketType::Tcp(inner) => Pin::new(inner).poll_write(cx, buf),
			SocketType::Tls(inner) => Pin::new(inner).poll_write(cx, buf),
			SocketType::Hbone(inner) => Pin::new(inner).poll_write(cx, buf),
			SocketType::Boxed(inner) => Pin::new(inner).poll_write(cx, buf),
		}
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
		match self.get_mut() {
			SocketType::Tcp(inner) => Pin::new(inner).poll_flush(cx),
			SocketType::Tls(inner) => Pin::new(inner).poll_flush(cx),
			SocketType::Hbone(inner) => Pin::new(inner).poll_flush(cx),
			SocketType::Boxed(inner) => Pin::new(inner).poll_flush(cx),
		}
	}

	fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
		match self.get_mut() {
			SocketType::Tcp(inner) => Pin::new(inner).poll_shutdown(cx),
			SocketType::Tls(inner) => Pin::new(inner).poll_shutdown(cx),
			SocketType::Hbone(inner) => Pin::new(inner).poll_shutdown(cx),
			SocketType::Boxed(inner) => Pin::new(inner).poll_shutdown(cx),
		}
	}

	fn poll_write_vectored(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>],
	) -> Poll<Result<usize, std::io::Error>> {
		match self.get_mut() {
			SocketType::Tcp(inner) => Pin::new(inner).poll_write_vectored(cx, bufs),
			SocketType::Tls(inner) => Pin::new(inner).poll_write_vectored(cx, bufs),
			SocketType::Hbone(inner) => Pin::new(inner).poll_write_vectored(cx, bufs),
			SocketType::Boxed(inner) => Pin::new(inner).poll_write_vectored(cx, bufs),
		}
	}

	fn is_write_vectored(&self) -> bool {
		match &self {
			SocketType::Tcp(inner) => inner.is_write_vectored(),
			SocketType::Tls(inner) => inner.is_write_vectored(),
			SocketType::Hbone(inner) => inner.is_write_vectored(),
			SocketType::Boxed(inner) => inner.is_write_vectored(),
		}
	}
}

impl AsyncRead for Socket {
	fn poll_read(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut ReadBuf<'_>,
	) -> Poll<std::io::Result<()>> {
		Pin::new(&mut self.inner).poll_read(cx, buf)
	}
}
impl AsyncWrite for Socket {
	fn poll_write(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<Result<usize, Error>> {
		Pin::new(&mut self.inner).poll_write(cx, buf)
	}

	fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
		Pin::new(&mut self.inner).poll_flush(cx)
	}

	fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
		Pin::new(&mut self.inner).poll_shutdown(cx)
	}

	fn poll_write_vectored(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>],
	) -> Poll<Result<usize, Error>> {
		Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
	}

	fn is_write_vectored(&self) -> bool {
		self.inner.is_write_vectored()
	}
}

#[derive(Debug, Clone)]
pub enum Extension {
	Single(http::Extensions),
	Wrapped(http::Extensions, Arc<Extension>),
}

impl Default for Extension {
	fn default() -> Self {
		Self::new()
	}
}

impl Extension {
	pub fn new() -> Self {
		Extension::Single(http::Extensions::new())
	}
	fn wrap(ext: Arc<Extension>) -> Self {
		Extension::Wrapped(http::Extensions::new(), ext)
	}

	pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
		match self {
			Extension::Single(extensions) => extensions.insert(val),
			Extension::Wrapped(extensions, _) => extensions.insert(val),
		}
	}
	pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
		match self {
			Extension::Single(extensions) => extensions.get::<T>(),
			Extension::Wrapped(extensions, inner) => {
				if let Some(got) = extensions.get::<T>() {
					Some(got)
				} else {
					inner.get::<T>()
				}
			},
		}
	}
}

fn to_canonical(addr: SocketAddr) -> SocketAddr {
	// another match has to be used for IPv4 and IPv6 support
	let ip = addr.ip().to_canonical();
	SocketAddr::from((ip, addr.port()))
}
