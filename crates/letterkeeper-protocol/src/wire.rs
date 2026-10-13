// SPDX-License-Identifier: GPL-3.0-only
//! Bounds run before upstream parsers receive response bytes.
use std::{
    fmt, io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
pub(crate) const MAX_WIRE: usize = 8 * 1024 * 1024;
pub(crate) const MAX_RAW: usize = 2 * 1024 * 1024;
pub(crate) const MAX_HEADER: usize = 64 * 1024;
#[derive(Default)]
pub(crate) struct State {
    pub literal_limit: AtomicUsize,
    pub exceeded: AtomicBool,
    pub append: AtomicBool,
    pub continuation: AtomicBool,
    pub submitted: AtomicBool,
}
pub(crate) struct Wire {
    inner: tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    pub state: Arc<State>,
    total: usize,
    line: Vec<u8>,
    literal: usize,
    imap: bool,
}
impl fmt::Debug for Wire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BoundedTlsStream")
    }
}
impl Wire {
    pub fn new(
        inner: tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
        state: Arc<State>,
        imap: bool,
    ) -> Self {
        state.literal_limit.store(MAX_HEADER, Ordering::Relaxed);
        Self {
            inner,
            state,
            total: 0,
            line: Vec::new(),
            literal: 0,
            imap,
        }
    }
    fn inspect(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.total = self.total.saturating_add(bytes.len());
        if self.total > MAX_WIRE {
            return self.limit();
        }
        for &b in bytes {
            if self.literal > 0 {
                self.literal -= 1;
                continue;
            }
            self.line.push(b);
            if self.line.len() > if self.imap { MAX_HEADER } else { 4096 } {
                return self.limit();
            }
            if b == b'\n' {
                if self.imap
                    && self.line.ends_with(b"}\r\n")
                    && let Some(pos) = self.line.iter().rposition(|b| *b == b'{')
                {
                    let digits = &self.line[pos + 1..self.line.len() - 3];
                    let digits = digits.strip_suffix(b"+").unwrap_or(digits);
                    let count = std::str::from_utf8(digits)
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok());
                    match count {
                        Some(n) if n <= self.state.literal_limit.load(Ordering::Relaxed) => {
                            self.literal = n
                        }
                        _ => return self.limit(),
                    }
                }
                if self.state.append.load(Ordering::Relaxed) && self.line.starts_with(b"+") {
                    self.state.continuation.store(true, Ordering::Relaxed);
                }
                self.line.clear();
            }
        }
        Ok(())
    }
    fn limit<T>(&self) -> io::Result<T> {
        self.state.exceeded.store(true, Ordering::Relaxed);
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protocol response limit",
        ))
    }
}
impl AsyncRead for Wire {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if out.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut scratch = [0u8; 4096];
        let len = out.remaining().min(scratch.len());
        let mut input = ReadBuf::new(&mut scratch[..len]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut input) {
            Poll::Ready(Ok(())) => {
                if let Err(e) = self.inspect(input.filled()) {
                    return Poll::Ready(Err(e));
                }
                out.put_slice(input.filled());
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}
impl AsyncWrite for Wire {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.state.append.load(Ordering::Relaxed)
            && self.state.continuation.load(Ordering::Relaxed)
            && !bytes.is_empty()
        {
            self.state.submitted.store(true, Ordering::Relaxed);
        }
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
impl lettre::transport::smtp::client::AsyncTokioStream for Wire {
    fn peer_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.inner.get_ref().0.peer_addr()
    }
}
