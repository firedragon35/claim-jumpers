//! A client session (cj-net) flying against a real server over renet's
//! in-memory local client: it sees only what its snapshots show it,
//! predicts its own tug, and has to hook a piece of salvage and bank it.

use cj_net::channel;
use cj_net::client::Session;
use cj_server::{Config, Server};
use cj_sim::consts::{BAY_X, BAY_Y, STEP};
use cj_sim::items::ItemType;
use cj_sim::world::World;
use cj_sim::{Action, Input};
use renet::RenetClient;
use std::time::Duration;

/// A client: its session, its connection and the world it draws.
struct Pilot {
    session: Session,
    client: RenetClient,
    seen: World,
}

impl Pilot {
    fn send(&mut self, (ch, bytes): (u8, Vec<u8>)) {
        self.client.send_message(ch, bytes);
    }
}

/// One step: the client acts, packets move, the server steps, and the
/// client takes in and draws what came back.
fn step(s: &mut Server, me: &mut Pilot, input: Input) {
    let dt = Duration::from_secs_f64(STEP);
    if let Some(msg) = me.session.step(input) {
        me.send(msg);
    }
    me.client.update(dt);
    s.renet
        .process_local_client(1, &mut me.client)
        .expect("local client");
    s.update(dt);
    s.renet
        .process_local_client(1, &mut me.client)
        .expect("local client");
    for ch in [channel::RELIABLE, channel::UNRELIABLE] {
        while let Some(bytes) = me.client.receive_message(ch) {
            me.session.receive(&bytes).expect("valid server message");
        }
    }
    me.seen.events.clear();
    me.session.present(&mut me.seen, STEP);
}

/// Held controls that steer the tug toward (x, y) at `thrust`.
fn steer(w: &World, x: f64, y: f64, thrust: f64) -> Input {
    let s = &w.me().ship;
    let want = (y - s.b.y).atan2(x - s.b.x);
    let mut off = want - s.angle;
    while off > std::f64::consts::PI {
        off -= std::f64::consts::TAU;
    }
    while off < -std::f64::consts::PI {
        off += std::f64::consts::TAU;
    }
    Input {
        turn: (off * 3.0).clamp(-1.0, 1.0),
        thrust: if off.abs() < 0.6 { thrust } else { 0.0 },
        reverse: 0.0,
        ..Input::default()
    }
}

#[test]
fn a_client_hooks_salvage_and_banks_it() {
    let mut s = Server::new(Config {
        version: "9.9.9".to_owned(),
        max_players: 4,
        seed: 3,
        record: false,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
    });
    // Clear the shift's own salvage and rivals so the test is about this
    // one piece, out of the bay tractor's reach.
    s.world.items.clear();
    s.world.jumpers.clear();
    s.world.jumper_timer = f64::INFINITY;
    let piece = s
        .world
        .new_item(ItemType::Scrap, BAY_X - 500.0, BAY_Y, 0.0, 0.0);
    let id = piece.id;
    s.world.items.push(piece);

    let mut me = Pilot {
        session: Session::new(),
        client: s.renet.new_local_client(1),
        seen: World::empty(cj_sim::rng::Rng::Constant(0.5), (1280.0, 720.0)),
    };
    let hello = me
        .session
        .hello("9.9.9", [1; 16], [2; 16], "Tester", (1280, 720));
    me.send(hello);

    let mut hooked = false;
    let mut worst: f64 = 0.0;
    for tick in 0..2400 {
        let input = if !hooked {
            // Out to the piece, hooking once it's close.
            if let Some(it) = me.seen.items.iter().find(|it| it.id == id) {
                let (x, y) = (it.b.x, it.b.y);
                let s = &me.seen.me().ship.b;
                if cj_sim::js::dist(s.x, s.y, x, y) < 110.0 {
                    let a = me.session.action(Action::Hook);
                    me.send(a);
                }
                steer(&me.seen, x, y, 0.6)
            } else {
                Input::default()
            }
        } else {
            // Home to the bay with it.
            steer(&me.seen, BAY_X, BAY_Y, 0.7)
        };
        step(&mut s, &mut me, input);
        hooked = hooked || me.seen.me().chain.contains(&id);
        // With no delay on the wire, the prediction should agree with the
        // server closely (it restarts from each snapshot and replays).
        if me.session.pilot.is_some() && tick > 10 {
            let (a, b) = (me.seen.me().ship.b, s.world.pilots[0].ship.b);
            worst = worst.max(cj_sim::js::dist(a.x, a.y, b.x, b.y));
        }
        if s.world.pilots[0].banked > 0.0 {
            assert!(hooked, "banked only after hooking");
            assert!(tick > 60, "it took a real flight");
            println!("worst prediction error {worst:.4} units over {tick} steps");
            assert!(worst < 1.0, "prediction strayed {worst} units");
            return;
        }
    }
    panic!(
        "never banked the piece (hooked: {hooked}, tug at {:?})",
        s.world.pilots[0].ship.b
    );
}

/// A link that holds every packet back `delay` steps each way, plus up to
/// `jitter` more at random (so packets can arrive out of order), and loses
/// `loss` percent of them. Its dice are seeded, so a test runs the same
/// every time.
struct Lagged {
    delay: u32,
    jitter: u32,
    loss: u32,
    dice: u64,
    up: Vec<(u32, Vec<u8>)>,
    down: Vec<(u32, Vec<u8>)>,
}

impl Lagged {
    /// The next of the link's dice, 0 to n - 1.
    fn roll(&mut self, n: u32) -> u32 {
        self.dice = self
            .dice
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.dice >> 33) % u64::from(n.max(1))) as u32
    }

    /// When a packet sent at step `t` arrives, or None if it is lost.
    fn arrival(&mut self, t: u32) -> Option<u32> {
        if self.roll(100) < self.loss {
            return None;
        }
        Some(t + self.delay + self.roll(self.jitter + 1))
    }
}

/// Takes the packets in `queue` that have arrived by step `t`.
fn due(queue: &mut Vec<(u32, Vec<u8>)>, t: u32) -> Vec<Vec<u8>> {
    let (now, later): (Vec<_>, Vec<_>) = queue.drain(..).partition(|(at, _)| *at <= t);
    *queue = later;
    now.into_iter().map(|(_, p)| p).collect()
}

/// A client and server on a fresh shift, joined over a lagged link with
/// `delay` steps each way, the rivals kept away.
fn lagged_pair(delay: u32) -> (Server, Pilot, Lagged) {
    rough_pair(delay, 0, 0)
}

/// As [`lagged_pair`], over a link that also jitters and loses packets.
fn rough_pair(delay: u32, jitter: u32, loss: u32) -> (Server, Pilot, Lagged) {
    let mut s = Server::new(Config {
        version: "9.9.9".to_owned(),
        max_players: 4,
        seed: 3,
        record: false,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
    });
    s.world.jumpers.clear();
    s.world.jumper_timer = f64::INFINITY;
    let mut me = Pilot {
        session: Session::new(),
        client: s.renet.new_local_client(1),
        seen: World::empty(cj_sim::rng::Rng::Constant(0.5), (1280.0, 720.0)),
    };
    let hello = me
        .session
        .hello("9.9.9", [1; 16], [2; 16], "Lag", (1280, 720));
    me.send(hello);
    let link = Lagged {
        delay,
        jitter,
        loss,
        dice: 0x5eed,
        up: Vec::new(),
        down: Vec::new(),
    };
    (s, me, link)
}

/// One step at step number `t` over the lagged link.
fn lagged_step(s: &mut Server, me: &mut Pilot, link: &mut Lagged, input: Input, t: u32) {
    let dt = Duration::from_secs_f64(STEP);
    if let Some(msg) = me.session.step(input) {
        me.send(msg);
    }
    me.client.update(dt);
    for p in me.client.get_packets_to_send() {
        if let Some(at) = link.arrival(t) {
            link.up.push((at, p));
        }
    }
    for p in due(&mut link.up, t) {
        s.renet.process_packet_from(&p, 1).expect("client");
    }
    s.update(dt);
    for p in s.renet.get_packets_to_send(1).expect("client") {
        if let Some(at) = link.arrival(t) {
            link.down.push((at, p));
        }
    }
    for p in due(&mut link.down, t) {
        me.client.process_packet(&p);
    }
    for ch in [channel::RELIABLE, channel::UNRELIABLE] {
        while let Some(bytes) = me.client.receive_message(ch) {
            me.session.receive(&bytes).expect("valid");
        }
    }
    me.seen.events.clear();
    me.session.present(&mut me.seen, STEP);
}

/// With a 130 ms round trip, the drawn tug only jumps a little when the
/// server corrects its prediction, flying a weaving course.
#[test]
fn corrections_under_lag_are_small() {
    let (mut s, mut me, mut link) = lagged_pair(4);
    let mut last: Option<cj_sim::entities::Body> = None;
    let mut worst = 0.0f64;
    for t in 0..900u32 {
        let input = Input {
            thrust: 0.8,
            turn: (f64::from(t) * STEP * 1.5).sin(),
            ..Input::default()
        };
        lagged_step(&mut s, &mut me, &mut link, input, t);
        if me.session.pilot.is_none() {
            continue;
        }
        let b = me.seen.me().ship.b;
        if let Some(prev) = last.filter(|_| t > 120) {
            // How far it is from where it would be moving smoothly.
            let (ex, ey) = (prev.x + prev.vx * STEP, prev.y + prev.vy * STEP);
            worst = worst.max(cj_sim::js::dist(b.x, b.y, ex, ey));
        }
        last = Some(b);
    }
    println!("worst correction at 130 ms: {worst:.3} units");
    assert!(worst < 6.0, "the tug jumped {worst} units");
}

/// With a 130 ms round trip, a shot appears the moment it is fired, along
/// the predicted heading, and the server's copy never doubles it.
#[test]
fn shots_leave_at_once_along_the_predicted_heading() {
    let (mut s, mut me, mut link) = lagged_pair(4);
    let turning = Input {
        turn: 1.0,
        ..Input::default()
    };
    for t in 0..120 {
        lagged_step(&mut s, &mut me, &mut link, turning, t);
    }
    assert!(me.session.pilot.is_some(), "joined");
    let firing = Input {
        turn: 1.0,
        fire: true,
        ..Input::default()
    };
    lagged_step(&mut s, &mut me, &mut link, firing, 120);
    let angle = me.seen.me().ship.angle;
    let shot = me
        .seen
        .bullets
        .first()
        .expect("the shot shows on the step it's fired");
    let heading = shot.vy.atan2(shot.vx);
    let off = (heading - angle).sin().abs();
    assert!(
        off < 0.2,
        "the shot leaves along the predicted heading ({off})"
    );
    for t in 121..240 {
        lagged_step(&mut s, &mut me, &mut link, firing, t);
        assert_eq!(
            me.seen.bullets.len(),
            me.session.shots().len(),
            "only the guessed shots are drawn, never the server's copies too"
        );
    }
}

/// A client follows the server out of one shift, through the break, and
/// into the next: it sees the break, says it is ready, is welcomed again,
/// and predicts its tug on the new field from the new shift's snapshots.
#[test]
fn a_client_follows_the_server_into_the_next_shift() {
    let mut s = Server::new(Config {
        version: "9.9.9".to_owned(),
        max_players: 4,
        seed: 3,
        record: false,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
    });
    let mut me = Pilot {
        session: Session::new(),
        client: s.renet.new_local_client(1),
        seen: World::empty(cj_sim::rng::Rng::Constant(0.5), (1280.0, 720.0)),
    };
    let hello = me
        .session
        .hello("9.9.9", [1; 16], [2; 16], "Tester", (1280, 720));
    me.send(hello);
    let fly = Input {
        thrust: 1.0,
        ..Input::default()
    };
    for _ in 0..120 {
        step(&mut s, &mut me, fly);
    }
    let moved = me.seen.me().ship.b;
    s.world.time_left = 0.001;
    for _ in 0..30 {
        step(&mut s, &mut me, Input::default());
    }
    assert!(me.session.in_break(), "the client sees the break");
    let ready = me.session.ready();
    me.send(ready);
    for _ in 0..30 {
        step(&mut s, &mut me, Input::default());
    }
    assert!(!me.session.in_break(), "the next shift is on");
    assert_eq!(s.shift_no(), 2);
    assert_eq!(me.session.pilot, Some(0));
    let fresh = me.seen.me().ship.b;
    assert!(
        cj_sim::js::dist(fresh.x, fresh.y, BAY_X, BAY_Y)
            < cj_sim::js::dist(moved.x, moved.y, BAY_X, BAY_Y),
        "back at the bay on the new field"
    );
    assert!(me.session.step(fly).is_some(), "flying again");
}

/// Over a bad link (130 ms, up to 70 ms of jitter, 10% of packets lost each
/// way) the client stays connected, the server keeps taking its controls,
/// the drawn tug moves without big jumps, and a reliable message (a chat
/// line) still gets through and back.
#[test]
fn play_holds_up_over_jitter_and_packet_loss() {
    let (mut s, mut me, mut link) = rough_pair(4, 4, 10);
    let mut last: Option<cj_sim::entities::Body> = None;
    let (mut worst, mut said) = (0.0f64, false);
    for t in 0..900u32 {
        let input = Input {
            thrust: 0.8,
            turn: (f64::from(t) * STEP * 1.5).sin(),
            ..Input::default()
        };
        lagged_step(&mut s, &mut me, &mut link, input, t);
        if me.session.pilot.is_none() {
            continue;
        }
        if !said {
            let msg = me.session.say("still here").expect("a line");
            me.send(msg);
            said = true;
        }
        let b = me.seen.me().ship.b;
        if let Some(prev) = last.filter(|_| t > 120) {
            let (ex, ey) = (prev.x + prev.vx * STEP, prev.y + prev.vy * STEP);
            worst = worst.max(cj_sim::js::dist(b.x, b.y, ex, ey));
        }
        last = Some(b);
    }
    println!("worst correction over a rough link: {worst:.3} units");
    assert!(s.renet.is_connected(1), "still connected");
    assert_eq!(me.session.pilot, Some(0));
    assert!(me.session.lobby.is_some(), "lobby lists arrive");
    assert!(
        me.session.chat.iter().any(|l| l.text == "still here"),
        "the reliable chat line came back"
    );
    assert!(worst < 6.0, "the tug jumped {worst} units");
}
