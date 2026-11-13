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
//! `CJ_NET_JITTER_MS` adds up to that much more at random, and
//! `CJ_NET_LOSS_PCT` drops that share of messages, each way. Jitter and loss
//! touch only the unreliable channel (inputs and snapshots): reliable
//! messages keep renet's promise of order and delivery.

use cj_net::client::Session;
use renet::{ConnectionConfig, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport, NetcodeTransportError};
use std::hash::{BuildHasher, Hasher};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

/// The file in the save folder that holds this install's multiplayer
/// identity: the player id and its secret, as 64 hex digits.
const ID_FILE: &str = "mp_id.txt";

/// How long to wait for a server before giving up.
const CONNECT_TIMEOUT: f64 = 8.0;
/// Seconds between Queries while looking at a server before joining.
const QUERY_EVERY: f64 = 2.0;

/// The server address for what was typed on the Join screen: `host` gets
/// the default port, and `host:port` keeps its own (a server started with
/// `--port`). A port that isn't a number is left for the lookup to reject.
pub fn server_addr(typed: &str) -> String {
    let typed = typed.trim();
    if typed.contains(':') {
        typed.to_owned()
    } else {
        format!("{typed}:{}", cj_net::DEFAULT_PORT)
    }
}

/// Looks `server` ("host:port") up on a worker thread and sends back its
/// first IPv4 address as "a.b.c.d:port", or the sentence to show. The OS
/// resolver has no timeout of its own and can take seconds on an unknown
/// name, so it must never run on the frame loop. Literal addresses answer
/// at once without a thread.
pub fn resolve(server: String) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    let literal = server_is_literal(&server);
    let look = move || {
        server
            .to_socket_addrs()
            .map_err(|e| format!("Can't find server {server}: {e}"))?
            .find(SocketAddr::is_ipv4)
            .map(|a| a.to_string())
            .ok_or_else(|| format!("Server {server} has no IPv4 address"))
    };
    if literal {
        let _ = tx.send(look());
    } else {
        // A dropped receiver (the player typed a new address) just ends it.
        std::thread::spawn(move || {
            let _ = tx.send(look());
        });
    }
    rx
}

/// An "a.b.c.d:port" address needs no name lookup.
fn server_is_literal(server: &str) -> bool {
    server.parse::<std::net::SocketAddrV4>().is_ok()
}

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
    /// Test-only network trouble (see the module notes).
    impair: Impair,
    /// Seconds since connecting, the clock for delayed messages.
    clock: f64,
    /// Messages waiting out the delay: (due time, channel, bytes).
    outgoing: std::collections::VecDeque<(f64, (u8, Vec<u8>))>,
    incoming: std::collections::VecDeque<(f64, Vec<u8>)>,
}

/// A test-only setting from the environment: a number in `0..=max`, or 0.
fn test_env(name: &str, max: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && (0.0..=max).contains(v))
        .unwrap_or(0.0)
}

/// Test-only network trouble, applied to each message each way.
#[derive(Clone, Copy, Debug, Default)]
struct Impair {
    /// Fixed delay each way, seconds.
    delay: f64,
    /// Up to this much more at random, seconds (unreliable channel only).
    jitter: f64,
    /// Share of messages dropped, 0..1 (unreliable channel only).
    loss: f64,
    /// xorshift state for the dice.
    seed: u64,
}

impl Impair {
    fn from_env() -> Impair {
        Impair {
            delay: test_env("CJ_NET_LAG_MS", 2000.0) / 2000.0,
            jitter: test_env("CJ_NET_JITTER_MS", 1000.0) / 1000.0,
            loss: test_env("CJ_NET_LOSS_PCT", 90.0) / 100.0,
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn active(&self) -> bool {
        self.delay > 0.0 || self.jitter > 0.0 || self.loss > 0.0
    }

    /// A uniform roll in 0..1.
    fn roll(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }

    /// When a message on channel `ch` handed over at `now` comes out, or
    /// None if it is lost.
    fn due(&mut self, now: f64, ch: u8) -> Option<f64> {
        if ch != cj_net::channel::UNRELIABLE {
            return Some(now + self.delay);
        }
        if self.loss > 0.0 && self.roll() < self.loss {
            return None;
        }
        Some(now + self.delay + self.jitter * self.roll())
    }
}

/// Takes the first queued message that is due by `now`. With jitter the
/// queue isn't in due order, so it is searched, not just peeked at the
/// front; queue order is kept among those due.
fn take_due<T>(queue: &mut std::collections::VecDeque<(f64, T)>, now: f64) -> Option<T> {
    let i = queue.iter().position(|(due, _)| *due <= now)?;
    queue.remove(i).map(|(_, m)| m)
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
            impair: Impair::from_env(),
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
                    if let Some(due) = self.impair.due(self.clock, ch) {
                        self.incoming.push_back((due, bytes.to_vec()));
                    }
                }
            }
            while let Some(bytes) = take_due(&mut self.incoming, self.clock) {
                // A server that sends nonsense is ignored, not trusted.
                let _ = self.session.receive(&bytes);
            }
            while let Some((ch, bytes)) = take_due(&mut self.outgoing, self.clock) {
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
    /// The smoothed round trip to the server, in milliseconds.
    pub fn rtt_ms(&self) -> f64 {
        self.client.rtt() * 1000.0
    }

    /// F3 overlay rows: round trip, packet loss, traffic, and how many
    /// inputs the server hasn't confirmed yet.
    pub fn net_rows(&self) -> [String; 3] {
        let n = self.client.network_info();
        [
            format!(
                "rtt {:.0} ms  loss {:.1}%",
                n.rtt * 1000.0,
                n.packet_loss * 100.0
            ),
            format!(
                "net {:.1} kB/s down {:.1} up",
                n.bytes_received_per_second / 1000.0,
                n.bytes_sent_per_second / 1000.0
            ),
            format!("unconfirmed inputs {}", self.session.unconfirmed()),
        ]
    }

    pub fn join(&mut self) {
        if let Some(h) = self.held.take() {
            self.hello = Some(h);
        }
    }

    /// Queues a message for the server.
    pub fn send(&mut self, (ch, bytes): (u8, Vec<u8>)) {
        if self.impair.active() {
            if let Some(due) = self.impair.due(self.clock, ch) {
                self.outgoing.push_back((due, (ch, bytes)));
            }
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

    /// Loss and jitter hit only the unreliable channel, at about the rate
    /// asked; the queue hands out messages once due, in queue order.
    #[test]
    fn impairment_drops_and_delays_only_unreliable_messages() {
        let mut imp = Impair {
            delay: 0.05,
            jitter: 0.02,
            loss: 0.1,
            seed: 1,
        };
        let un = cj_net::channel::UNRELIABLE;
        let rel = cj_net::channel::RELIABLE;
        let mut lost = 0;
        for _ in 0..10_000 {
            match imp.due(1.0, un) {
                None => lost += 1,
                Some(d) => assert!((1.05..=1.07).contains(&d), "{d}"),
            }
            assert_eq!(imp.due(1.0, rel), Some(1.05), "reliable: delay only");
        }
        assert!((800..1200).contains(&lost), "about 10% lost: {lost}");
        let mut q: std::collections::VecDeque<(f64, char)> =
            [(2.0, 'a'), (1.0, 'b'), (1.5, 'c')].into();
        assert_eq!(take_due(&mut q, 1.6), Some('b'));
        assert_eq!(take_due(&mut q, 1.6), Some('c'));
        assert_eq!(take_due(&mut q, 1.6), None);
    }

    /// A bare host gets the default port; a typed port is kept.
    #[test]
    fn a_typed_port_is_kept() {
        let d = cj_net::DEFAULT_PORT;
        assert_eq!(server_addr(" 192.0.2.10 "), format!("192.0.2.10:{d}"));
        assert_eq!(server_addr("192.0.2.10:5000"), "192.0.2.10:5000");
        assert_eq!(server_addr("host.example:5000"), "host.example:5000");
    }

    /// A literal address resolves at once; a bad one gives a sentence.
    #[test]
    fn literal_addresses_resolve_without_a_wait() {
        let rx = resolve("192.0.2.10:5000".to_owned());
        assert_eq!(rx.try_recv().unwrap().unwrap(), "192.0.2.10:5000");
        let rx = resolve("192.0.2.10:notaport".to_owned());
        let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(r.unwrap_err().starts_with("Can't find server"));
    }

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
        // Real time, not a frame count: a slow runner gets longer, not a
        // failure.
        let mut deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
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
            // Join the moment the server has answered the look, as a click on
            // the Join screen does. Waiting longer races the server's 5 s
            // Hello deadline on a slow machine.
            if guest.looking() && guest.session.info.is_some() {
                guest.join();
            }
            // Once both fly, keep going a while: they must stay connected.
            if host.session.pilot.is_some() && guest.session.pilot.is_some() {
                let now = std::time::Instant::now();
                deadline = deadline.min(now + Duration::from_secs(2));
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
