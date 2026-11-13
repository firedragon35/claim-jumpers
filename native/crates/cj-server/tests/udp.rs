//! The shared UDP loop (`bind_udp` and `run`, used by the headless binary
//! and by the game's Host server) over real loopback sockets: a client is
//! welcomed, stopping the server disconnects it, and a long stall is
//! skipped rather than caught up.

use cj_net::channel;
use cj_net::client::Session;
use cj_server::{bind_udp, run, Config, Server};
use renet::{ConnectionConfig, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// The test server's settings: version 9.9.9, four pilots, nothing saved.
fn config() -> Config {
    Config {
        version: "9.9.9".to_owned(),
        max_players: 4,
        seed: 1,
        record: false,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
        pvp: false,
    }
}

/// A server on a loopback port, run on its own thread until `stop` is set,
/// or until a step panics once `explode` is.
fn serve(
    stop: Arc<AtomicBool>,
    explode: Arc<AtomicBool>,
) -> (
    std::net::SocketAddr,
    std::thread::JoinHandle<Result<(), String>>,
) {
    let mut transport = bind_udp(Ipv4Addr::LOCALHOST, 0, 4).expect("bind");
    let addr = transport.addresses()[0];
    let mut server = Server::new(config());
    let thread = std::thread::spawn(move || {
        run(&mut transport, &mut server, |_| {
            assert!(!explode.load(Ordering::Relaxed), "a test failure");
            !stop.load(Ordering::Relaxed)
        })
    });
    (addr, thread)
}

/// A UDP client joins the server at `addr`, calls `welcomed` once it has a
/// pilot, then waits (up to 6 s in all) to be disconnected. Returns its
/// session, whether it was disconnected, and how long that took after
/// `welcomed`.
fn join_and_wait(addr: std::net::SocketAddr, welcomed: impl Fn()) -> (Session, bool, Duration) {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock");
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("client socket");
    let mut net = NetcodeClientTransport::new(
        now,
        ClientAuthentication::Unsecure {
            protocol_id: cj_net::PROTOCOL_ID,
            client_id: 77,
            server_addr: addr,
            user_data: None,
        },
        socket,
    )
    .expect("client transport");
    let mut client = RenetClient::new(ConnectionConfig::default());
    let mut session = Session::new();
    session.set_login("9.9.9", [5; 16], &[6; 16], "Udp", (1280, 720));
    let dt = Duration::from_millis(16);
    let start = Instant::now();
    let mut stopped_at = None;
    while start.elapsed() < Duration::from_secs(6) {
        client.update(dt);
        let _ = net.update(dt, &mut client);
        if client.is_connected() {
            for ch in [channel::RELIABLE, channel::UNRELIABLE] {
                while let Some(bytes) = client.receive_message(ch) {
                    session.receive(&bytes).expect("valid");
                }
            }
            while let Some((ch, bytes)) = session.take_login() {
                client.send_message(ch, bytes);
            }
        }
        if session.pilot.is_some() && stopped_at.is_none() {
            welcomed();
            stopped_at = Some(Instant::now());
        }
        if stopped_at.is_some() && client.is_disconnected() {
            break;
        }
        let _ = net.send_packets(&mut client);
        std::thread::sleep(dt);
    }
    let took = stopped_at.map_or(Duration::MAX, |at| at.elapsed());
    (session, client.is_disconnected(), took)
}

#[test]
fn a_udp_client_is_welcomed_and_dropped_when_the_server_stops() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, thread) = serve(stop.clone(), Arc::new(AtomicBool::new(false)));
    let (session, dropped, _) = join_and_wait(addr, || stop.store(true, Ordering::Relaxed));
    assert_eq!(thread.join().expect("the server thread ends"), Ok(()));
    assert_eq!(session.pilot, Some(0), "welcomed over UDP");
    assert!(dropped, "told the server stopped");
}

/// A server step that panics ends the run with the reason, and every
/// client is told at once rather than waiting for a timeout.
#[test]
fn a_failing_server_tells_its_clients_at_once() {
    let explode = Arc::new(AtomicBool::new(false));
    let (addr, thread) = serve(Arc::new(AtomicBool::new(false)), explode.clone());
    let (session, dropped, took) = join_and_wait(addr, || explode.store(true, Ordering::Relaxed));
    let ran = thread.join().expect("the panic is caught, not passed on");
    assert!(ran.is_err_and(|why| why.contains("a test failure")));
    assert_eq!(session.pilot, Some(0));
    assert!(dropped, "told the server stopped");
    assert!(
        took < Duration::from_secs(2),
        "at once, not by timeout ({took:?})"
    );
}

/// After a one-second stall the loop goes on at 60 Hz from where it is,
/// instead of running the 60 missed steps back to back.
#[test]
fn a_long_stall_is_skipped_not_caught_up() {
    let mut transport = bind_udp(Ipv4Addr::LOCALHOST, 0, 4).expect("bind");
    let mut server = Server::new(config());
    // When the stall ended, and the step the server was on then.
    let mut stalled: Option<(Instant, u32)> = None;
    let mut steps_after = 0;
    run(&mut transport, &mut server, |s| match stalled {
        None if s.tick() == 10 => {
            std::thread::sleep(Duration::from_secs(1));
            stalled = Some((Instant::now(), s.tick()));
            true
        }
        None => true,
        // Count the steps run in the 0.3 s after the stall, then stop.
        Some((at, tick)) => {
            steps_after = s.tick() - tick;
            at.elapsed() < Duration::from_millis(300)
        }
    })
    .expect("ran");
    // About 18 steps at 60 Hz; catching up would add the 60 missed ones.
    assert!(
        steps_after < 40,
        "{steps_after} steps in 0.3 s: the stall was caught up"
    );
    assert!(server.log.iter().any(|l| l.contains("skipping ahead")));
}
