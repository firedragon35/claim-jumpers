//! Playing on a server over UDP (CJN-16): renet's netcode client carrying a
//! cj-net client session.
//!
//! For now the way in is the `--join HOST:PORT` command-line option; the
//! Host and Join screens come with player hosting (CJN-19). The transport
//! is renet's unsecure netcode, so traffic (including the player secret)
//! is not encrypted.
//!
//! For testing on one machine, `CJ_NET_LAG_MS` holds every message back so
//! the round trip takes that long: half on the way out, half on the way in.

use cj_net::client::Session;
use renet::{ConnectionConfig, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport, NetcodeTransportError};
use std::hash::{BuildHasher, Hasher};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, SystemTime};

/// The file in the save folder that holds this install's multiplayer
/// identity: the player id and its secret, as 64 hex digits.
const ID_FILE: &str = "mp_id.txt";

/// How long to wait for a server before giving up.
const CONNECT_TIMEOUT: f64 = 8.0;
/// Seconds between Queries while looking at a server before joining.
const QUERY_EVERY: f64 = 2.0;

/// 16 bytes that are hard to guess: std's per-process hash keys come from
/// the operating system's random source. Not cryptographic, but the id and
/// secret only need to be unlikely to collide or be guessed.
fn random_bytes() -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = std::hash::RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

/// Reads 32 bytes from 64 hex digits.
fn from_hex(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// This install's player id and secret, made and saved on first use. A
/// server recognises a returning player by them; losing the file means
/// starting fresh on every server.
pub fn identity(store: &crate::save::Store) -> ([u8; 16], [u8; 16]) {
    if let Some(b) = store.read(ID_FILE).as_deref().and_then(from_hex) {
        let (id, secret) = b.split_at(16);
        return (
            id.try_into().expect("16 bytes"),
            secret.try_into().expect("16 bytes"),
        );
    }
    let (id, secret) = (random_bytes(), random_bytes());
    store.write(ID_FILE, &to_hex(&[id, secret].concat()));
    (id, secret)
}

/// A connection to a server and the shift seen through it.
pub struct Online {
    pub session: Session,
    client: RenetClient,
    transport: NetcodeClientTransport,
    /// The Hello, sent as soon as the connection is up.
    hello: Option<(u8, Vec<u8>)>,
    /// A Hello held back while the player looks at the server first
    /// ([`Online::join`] sends it).
    held: Option<(u8, Vec<u8>)>,
    /// Seconds until the next Query while looking first.
    query_in: f64,
    /// Seconds spent waiting to connect.
    waited: f64,
    /// The server as the player typed it, for messages.
    server: String,
    /// Why the session ended, once it has, as a sentence for the player.
    pub ended: Option<String>,
    /// Test-only added delay each way, in seconds (`CJ_NET_LAG_MS` / 2).
    delay: f64,
    /// Seconds since connecting, the clock for delayed messages.
    clock: f64,
    /// Messages waiting out the delay: (due time, channel, bytes).
    outgoing: std::collections::VecDeque<(f64, u8, Vec<u8>)>,
    incoming: std::collections::VecDeque<(f64, Vec<u8>)>,
}

/// The test-only added round trip, in seconds, from `CJ_NET_LAG_MS`.
fn test_lag() -> f64 {
    std::env::var("CJ_NET_LAG_MS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|ms| ms.is_finite() && (0.0..=2000.0).contains(ms))
        .map_or(0.0, |ms| ms / 1000.0)
}

/// The game's version: a server only plays with its own version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Online {
    /// Starts connecting to `server` (a host name or address with a port).
    /// With `look_first`, the client only asks how full the server is
    /// ([`Session::info`]) until [`Online::join`]; otherwise it joins at once.
    pub fn connect(
        server: &str,
        id: ([u8; 16], [u8; 16]),
        pilot: &str,
        view: (u16, u16),
        look_first: bool,
    ) -> Result<Online, String> {
        let addr: SocketAddr = server
            .to_socket_addrs()
            .map_err(|e| format!("Can't find server {server}: {e}"))?
            .find(SocketAddr::is_ipv4)
            .ok_or_else(|| format!("Server {server} has no IPv4 address"))?;
        let socket =
            UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| format!("Can't open a UDP socket: {e}"))?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        // A connection number the server tells clients apart by.
        let client_id = u64::from_le_bytes(random_bytes()[..8].try_into().expect("8 bytes"));
        let transport = NetcodeClientTransport::new(
            now,
            ClientAuthentication::Unsecure {
                protocol_id: cj_net::PROTOCOL_ID,
                client_id,
                server_addr: addr,
                user_data: None,
            },
            socket,
        )
        .map_err(|e| format!("Can't start the connection: {e}"))?;
        let session = Session::new();
        let hello = session.hello(VERSION, id.0, id.1, pilot, view);
        let (hello, held) = if look_first {
            (None, Some(hello))
        } else {
            (Some(hello), None)
        };
        Ok(Online {
            session,
            client: RenetClient::new(ConnectionConfig::default()),
            transport,
            hello,
            held,
            query_in: 0.0,
            waited: 0.0,
            server: server.to_owned(),
            ended: None,
            delay: test_lag() / 2.0,
            clock: 0.0,
            outgoing: std::collections::VecDeque::new(),
            incoming: std::collections::VecDeque::new(),
        })
    }

    /// Moves packets in and out for `dt` seconds and takes in what arrived.
    /// Call it every frame, whatever screen is up: it also sends what was
    /// queued since the last call, so the Join screen's queries and the
    /// lobby's Ready and chat reach the server, not only a shift's controls.
    pub fn pump(&mut self, dt: f64) {
        if self.ended.is_some() {
            return;
        }
        let d = Duration::from_secs_f64(dt);
        self.clock += dt;
        self.client.update(d);
        match self.transport.update(d, &mut self.client) {
            // Windows reports a UDP packet that found no one listening (yet)
            // as a reset connection. That isn't fatal: the server may still
            // be starting, and a server that is really gone times out.
            Err(NetcodeTransportError::IO(e))
                if e.kind() == std::io::ErrorKind::ConnectionReset => {}
            Err(e) => {
                self.ended = Some(format!("Lost the server: {e}"));
                return;
            }
            Ok(()) => {}
        }
        if self.client.is_connected() {
            if let Some((ch, bytes)) = self.hello.take() {
                self.client.send_message(ch, bytes);
            }
            // Looking first: ask again every 2 s, which also keeps the
            // server from dropping a connection that hasn't said Hello.
            if self.held.is_some() {
                self.query_in -= dt;
                if self.query_in <= 0.0 {
                    self.query_in = QUERY_EVERY;
                    let (ch, bytes) = self.session.query();
                    self.client.send_message(ch, bytes);
                }
            }
            for ch in [cj_net::channel::RELIABLE, cj_net::channel::UNRELIABLE] {
                while let Some(bytes) = self.client.receive_message(ch) {
                    self.incoming
                        .push_back((self.clock + self.delay, bytes.to_vec()));
                }
            }
            while self
                .incoming
                .front()
                .is_some_and(|(due, _)| *due <= self.clock)
            {
                let (_, bytes) = self.incoming.pop_front().expect("front exists");
                // A server that sends nonsense is ignored, not trusted.
                let _ = self.session.receive(&bytes);
            }
            while self
                .outgoing
                .front()
                .is_some_and(|(due, _, _)| *due <= self.clock)
            {
                let (_, ch, bytes) = self.outgoing.pop_front().expect("front exists");
                self.client.send_message(ch, bytes);
            }
        } else if self.client.is_disconnected() {
            self.ended = Some("The server closed the connection.".to_owned());
        } else {
            self.waited += dt;
            if self.waited > CONNECT_TIMEOUT {
                self.ended = Some(format!(
                    "No answer from a server at {}. Is it running? If it is, the host's firewall may be blocking UDP port 47820.",
                    self.server
                ));
            }
        }
        if let Some(r) = &self.session.rejected {
            self.ended = Some(r.reason(VERSION));
        }
        self.flush();
    }

    /// Whether this connection is still only looking at the server.
    pub fn looking(&self) -> bool {
        self.held.is_some()
    }

    /// Joins after looking first: sends the held Hello.
    pub fn join(&mut self) {
        if let Some(h) = self.held.take() {
            self.hello = Some(h);
        }
    }

    /// Queues a message for the server.
    pub fn send(&mut self, (ch, bytes): (u8, Vec<u8>)) {
        if self.delay > 0.0 {
            self.outgoing
                .push_back((self.clock + self.delay, ch, bytes));
        } else if self.client.is_connected() {
            self.client.send_message(ch, bytes);
        }
    }

    /// Sends everything queued this frame.
    pub fn flush(&mut self) {
        if self.client.is_connected() {
            let _ = self.transport.send_packets(&mut self.client);
        }
    }

    /// Says goodbye, so the server frees the slot at once.
    pub fn leave(&mut self) {
        self.transport.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two players reach a real UDP server the way the game does: the
    /// first joins at once (as the host does), the second looks the server
    /// up first and joins from the Join screen. Both must be flying.
    #[test]
    fn a_second_player_joins_after_looking_first() {
        let mut transport =
            cj_server::bind_udp(std::net::Ipv4Addr::LOCALHOST, 0, 4).expect("a free port");
        let port = transport.addresses()[0].port();
        let mut server = cj_server::Server::new(cj_server::Config {
            version: VERSION.to_owned(),
            max_players: 4,
            seed: 1,
            record: false,
            mode: cj_sim::career::Mode::Standard,
            careers: None,
            pvp: false,
        });
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let lines = log.clone();
        let thread = std::thread::spawn(move || {
            let _ = cj_server::run(&mut transport, &mut server, |s| {
                lines.lock().expect("log").append(&mut s.log);
                !flag.load(std::sync::atomic::Ordering::Relaxed)
            });
        });
        let addr = format!("127.0.0.1:{port}");
        let mut host =
            Online::connect(&addr, ([1; 16], [1; 16]), "Host", (1280, 720), false).expect("host");
        let mut guest =
            Online::connect(&addr, ([2; 16], [2; 16]), "Guest", (1280, 720), true).expect("guest");
        let frame = 1.0 / 60.0;
        for f in 0..60 * 8 {
            for on in [&mut host, &mut guest] {
                on.pump(frame);
                // As in the game: only a shift in play steps and flushes;
                // the Join screen and the lobby only pump.
                if on.looking() || on.session.pilot.is_none() {
                    continue;
                }
                if let Some(msg) = on.session.step(cj_sim::Input::default()) {
                    on.send(msg);
                }
                on.flush();
            }
            if guest.looking() && guest.session.info.is_some() && f > 60 * 3 {
                guest.join();
            }
            std::thread::sleep(Duration::from_secs_f64(frame));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        thread.join().expect("server thread");
        let log = log.lock().expect("log").join(
            "
",
        );
        for (who, on) in [("host", &host), ("guest", &guest)] {
            assert!(
                on.ended.is_none(),
                "{who} ended: {:?}
server log:
{log}",
                on.ended
            );
        }
        assert_eq!(
            host.session.pilot,
            Some(0),
            "server log:
{log}"
        );
        assert_eq!(
            guest.session.pilot,
            Some(1),
            "server log:
{log}"
        );
    }

    /// The identity survives its file's round trip; a bad file is refused.
    #[test]
    fn identities_round_trip_through_hex() {
        let bytes: Vec<u8> = (0..32).collect();
        let text = to_hex(&bytes);
        assert_eq!(text.len(), 64);
        assert_eq!(from_hex(&text).map(|b| b.to_vec()), Some(bytes));
        assert_eq!(from_hex("not hex at all"), None);
        assert_eq!(from_hex(&"zz".repeat(32)), None);
    }

    /// Two ids made in a row differ.
    #[test]
    fn random_ids_differ() {
        assert_ne!(random_bytes(), random_bytes());
    }
}
