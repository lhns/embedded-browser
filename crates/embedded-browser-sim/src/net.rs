//! `Net` for the simulator: `file:` from disk, `http(s):` over ureq (rustls), each request
//! on its own thread so `fetch` never blocks the engine thread.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, channel};

use embedded_browser::{FileNet, Method, Net, NetError, ReqId, Request, Response};

type Completion = (ReqId, Result<Response, NetError>);

pub struct SimNet {
    next: u64,
    agent: ureq::Agent,
    local: VecDeque<Completion>,
    tx: Sender<Completion>,
    rx: Receiver<Completion>,
    cancelled: Vec<ReqId>,
}

impl Default for SimNet {
    fn default() -> Self {
        let (tx, rx) = channel();
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .user_agent("embedded-browser-sim/0.0")
            .build()
            .into();
        Self {
            next: 0,
            agent,
            local: VecDeque::new(),
            tx,
            rx,
            cancelled: Vec::new(),
        }
    }
}

impl Net for SimNet {
    fn fetch(&mut self, request: Request) -> ReqId {
        self.next += 1;
        let id = ReqId(self.next);
        if request.url.starts_with("http://") || request.url.starts_with("https://") {
            let agent = self.agent.clone();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send((id, http(&agent, &request)));
            });
        } else {
            let result = match request.method {
                Method::Get => FileNet::read(&request.url, request.max_bytes),
                Method::Post => Err(NetError::Unsupported("POST to file:".into())),
            };
            self.local.push_back((id, result));
        }
        id
    }

    fn poll(&mut self) -> Option<Completion> {
        loop {
            let next = self.local.pop_front().or_else(|| self.rx.try_recv().ok())?;
            if let Some(i) = self.cancelled.iter().position(|c| *c == next.0) {
                self.cancelled.swap_remove(i);
                continue;
            }
            return Some(next);
        }
    }

    fn cancel(&mut self, id: ReqId) {
        self.cancelled.push(id);
    }
}

fn http(agent: &ureq::Agent, request: &Request) -> Result<Response, NetError> {
    use ureq::ResponseExt;
    let failed = |e: ureq::Error| match e {
        ureq::Error::BodyExceedsLimit(_) => NetError::TooLarge,
        e => NetError::Failed(e.to_string()),
    };
    let mut response = match request.method {
        Method::Get => {
            let mut r = agent.get(&request.url);
            for (k, v) in &request.headers {
                r = r.header(k, v);
            }
            r.call()
        }
        Method::Post => {
            let mut r = agent.post(&request.url);
            for (k, v) in &request.headers {
                r = r.header(k, v);
            }
            r.send(&request.body[..])
        }
    }
    .map_err(failed)?;
    let url = response.get_uri().to_string();
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = response
        .body_mut()
        .with_config()
        .limit(request.max_bytes as u64)
        .read_to_vec()
        .map_err(failed)?;
    Ok(Response {
        url,
        status,
        content_type,
        body,
    })
}
