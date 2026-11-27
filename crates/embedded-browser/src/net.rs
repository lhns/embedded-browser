//! The non-blocking network seam (ADR 0003).

use std::collections::VecDeque;

/// Identifies a request issued through [`Net::fetch`]. Chosen by the [`Net`] implementation;
/// it must be unique among the requests in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReqId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Request {
    /// Absolute URL.
    pub url: String,
    pub method: Method,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The response body may not exceed this; the implementation should stop reading and
    /// fail with [`NetError::TooLarge`] instead of buffering more.
    pub max_bytes: usize,
}

impl Request {
    pub fn get(url: impl Into<String>, max_bytes: usize) -> Self {
        Self {
            url: url.into(),
            method: Method::Get,
            headers: Vec::new(),
            body: Vec::new(),
            max_bytes,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Response {
    /// The final URL, after redirects.
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    /// Decoded body (after any content encoding).
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetError {
    /// The body exceeded [`Request::max_bytes`].
    TooLarge,
    /// The scheme or method is not supported by this implementation.
    Unsupported(String),
    /// DNS, connection, TLS, I/O or protocol failure.
    Failed(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::TooLarge => f.write_str("response too large"),
            NetError::Unsupported(s) => write!(f, "unsupported: {s}"),
            NetError::Failed(s) => write!(f, "failed: {s}"),
        }
    }
}

/// Network access for documents and subresources, polled from the engine thread.
///
/// `fetch` must not block: it starts the request and returns. Completions are returned by
/// `poll`, which [`Browser::tick`](crate::Browser::tick) drains. Every fetched id must
/// complete exactly once (success or error) unless it is cancelled.
pub trait Net {
    fn fetch(&mut self, request: Request) -> ReqId;

    /// The next finished request, if any.
    fn poll(&mut self) -> Option<(ReqId, Result<Response, NetError>)>;

    /// The engine no longer wants this response (e.g. after a new navigation).
    fn cancel(&mut self, _id: ReqId) {}
}

/// A [`Net`] serving `file:` URLs from the local file system (an SD card on the device,
/// the corpus on the host). Reads complete on the next `poll`.
#[derive(Default)]
pub struct FileNet {
    next: u64,
    done: VecDeque<(ReqId, Result<Response, NetError>)>,
}

impl FileNet {
    /// Read a `file:` URL synchronously, honouring `max_bytes`.
    pub fn read(url: &str, max_bytes: usize) -> Result<Response, NetError> {
        let parsed = url::Url::parse(url).map_err(|e| NetError::Failed(e.to_string()))?;
        if parsed.scheme() != "file" {
            return Err(NetError::Unsupported(parsed.scheme().to_owned()));
        }
        let path = parsed
            .to_file_path()
            .map_err(|()| NetError::Failed(format!("not a local path: {url}")))?;
        let len = std::fs::metadata(&path)
            .map_err(|e| NetError::Failed(e.to_string()))?
            .len();
        if len > max_bytes as u64 {
            return Err(NetError::TooLarge);
        }
        let body = std::fs::read(&path).map_err(|e| NetError::Failed(e.to_string()))?;
        Ok(Response {
            url: url.to_owned(),
            status: 200,
            content_type: None,
            body,
        })
    }
}

impl Net for FileNet {
    fn fetch(&mut self, request: Request) -> ReqId {
        self.next += 1;
        let id = ReqId(self.next);
        let result = match request.method {
            Method::Get => Self::read(&request.url, request.max_bytes),
            Method::Post => Err(NetError::Unsupported("POST to file:".into())),
        };
        self.done.push_back((id, result));
        id
    }

    fn poll(&mut self) -> Option<(ReqId, Result<Response, NetError>)> {
        self.done.pop_front()
    }

    fn cancel(&mut self, id: ReqId) {
        self.done.retain(|(i, _)| *i != id);
    }
}
