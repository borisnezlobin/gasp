//! The app's half of the bridge: a socket that answers the server.
//!
//! [`BridgeListener::start`] binds the vault's endpoint, writes a fresh
//! token beside it and blocks in `accept` on its own thread, so an idle
//! app spends nothing on it. Each connection carries one request, which
//! goes to the app's handler on that thread; the handler hands it to the
//! main thread and waits for the answer. Dropping the listener removes
//! the socket and token and lets the thread end.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;

use super::endpoint::{Endpoint, EndpointInfo, new_token, tokens_match};
use super::{Envelope, MAX_LINE_BYTES, Reply, Request};

#[cfg(unix)]
type Listener = std::os::unix::net::UnixListener;
#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;
#[cfg(not(unix))]
type Listener = std::net::TcpListener;
#[cfg(not(unix))]
type Stream = std::net::TcpStream;

/// How long a client gets to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Answers one request: a JSON result, or why it failed in words.
pub type Handler = dyn Fn(Request) -> Result<Value, String> + Send;

/// A running bridge. Dropping it stops it.
pub struct BridgeListener {
    endpoint: Endpoint,
    port: Option<u16>,
    stopped: Arc<AtomicBool>,
}

impl BridgeListener {
    /// Starts listening at `endpoint`. Fails when another app already
    /// answers there, such as a second window on the same vault.
    pub fn start(
        endpoint: Endpoint,
        handler: impl Fn(Request) -> Result<Value, String> + Send + 'static,
    ) -> io::Result<BridgeListener> {
        let (listener, port) = bind(&endpoint)?;
        let token = new_token()?;
        let info = EndpointInfo {
            token: token.clone(),
            port,
        };
        if let Err(error) = endpoint.write_info(&info) {
            endpoint.remove();
            return Err(error);
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let handler: Box<Handler> = Box::new(handler);
        std::thread::Builder::new()
            .name("mcp-bridge".into())
            .spawn(move || accept_loop(&listener, &token, &handler, &stop))?;
        Ok(BridgeListener {
            endpoint,
            port,
            stopped,
        })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Drop for BridgeListener {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        // Wake the thread out of `accept` so it sees the flag. A request
        // being answered finishes first; nothing here waits for it.
        let woken = connect_to(&self.endpoint, self.port);
        drop(woken);
        self.endpoint.remove();
    }
}

fn accept_loop(listener: &Listener, token: &str, handler: &Handler, stopped: &AtomicBool) {
    for stream in listener.incoming() {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        match stream {
            Ok(stream) => serve(stream, token, handler),
            Err(error) => eprintln!("mcp bridge: {error}"),
        }
    }
}

/// Reads one request, answers it if the token is right, and closes.
fn serve(stream: Stream, token: &str, handler: &Handler) {
    // Accepted sockets can inherit non-blocking mode on some platforms.
    stream.set_nonblocking(false).ok();
    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
    let reply = match read_envelope(&stream) {
        Ok(envelope) if tokens_match(&envelope.token, token) => {
            Reply::from_result(handler(envelope.request))
        }
        // A wrong token gets no answer at all.
        Ok(_) => return,
        Err(message) => Reply::from_result(Err(message)),
    };
    if let Err(error) = write_reply(&stream, &reply) {
        eprintln!("mcp bridge: couldn't answer: {error}");
    }
}

fn read_envelope(stream: &Stream) -> Result<Envelope, String> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE_BYTES))
        .read_line(&mut line)
        .map_err(|error| format!("couldn't read the request: {error}"))?;
    serde_json::from_str(&line).map_err(|error| format!("that isn't a request: {error}"))
}

fn write_reply(mut stream: &Stream, reply: &Reply) -> io::Result<()> {
    let mut line = serde_json::to_vec(reply).map_err(io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()
}

/// Binds the vault's socket, clearing one a crashed app left behind.
#[cfg(unix)]
fn bind(endpoint: &Endpoint) -> io::Result<(Listener, Option<u16>)> {
    use std::os::unix::fs::PermissionsExt;
    endpoint.prepare_dir()?;
    let socket = endpoint.socket_path();
    if std::fs::symlink_metadata(&socket).is_ok() {
        if Stream::connect(&socket).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another window already serves this vault",
            ));
        }
        std::fs::remove_file(&socket)?;
    }
    let listener = Listener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, None))
}

/// Binds a port on localhost only; the token file says which.
#[cfg(not(unix))]
fn bind(endpoint: &Endpoint) -> io::Result<(Listener, Option<u16>)> {
    endpoint.prepare_dir()?;
    let listener = Listener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    Ok((listener, Some(port)))
}

/// Connects to the endpoint, as the server does.
#[cfg(unix)]
pub(super) fn connect_to(endpoint: &Endpoint, _port: Option<u16>) -> io::Result<Stream> {
    Stream::connect(endpoint.socket_path())
}

#[cfg(not(unix))]
pub(super) fn connect_to(_endpoint: &Endpoint, port: Option<u16>) -> io::Result<Stream> {
    let port = port.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no port"))?;
    Stream::connect((std::net::Ipv4Addr::LOCALHOST, port))
}

/// Sends one request line and reads the answer, as the server does.
pub(super) fn exchange(mut stream: Stream, line: &[u8], timeout: Duration) -> io::Result<String> {
    stream.set_read_timeout(Some(timeout))?;
    stream.write_all(line)?;
    stream.flush()?;
    let mut answer = String::new();
    BufReader::new(stream.take(MAX_LINE_BYTES)).read_line(&mut answer)?;
    Ok(answer)
}
