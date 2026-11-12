//! The server end to end over renet's in-memory local clients (the same
//! route the game's host takes): handshakes and refusals, hostile and
//! flooding clients, reconnecting, late joins, two and three pilots in one
//! shift, the per-pilot view, shared sounds, PvP between three players,
//! and exact replay from a recording.

use cj_net::{channel, ClientMsg, Reject, ServerMsg, Snapshot};
use cj_server::{replay, Config, Server};
use cj_sim::consts::STEP;
use cj_sim::events::{Emitted, Event, Sfx};
use cj_sim::perm::Pvp;
use cj_sim::{Action, Input};
use renet::{ClientId, RenetClient};
use std::time::Duration;

const VERSION: &str = "9.9.9";

fn server(max_players: usize) -> Server {
    Server::new(config_for(max_players))
}

/// The test server's settings: recorded, Standard mode, no saved careers,
/// co-op only.
fn config_for(max_players: usize) -> Config {
    Config {
        version: VERSION.to_owned(),
        max_players,
        seed: 42,
        record: true,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
        pvp: false,
    }
}

/// A scripted client.
struct Bot {
    id: ClientId,
    client: RenetClient,
    player: [u8; 16],
    pilot: Option<u8>,
    rejected: Option<Reject>,
    /// Told to wait in the lobby for the next shift.
    waiting: bool,
    info: Option<cj_net::Info>,
    lobby: Option<cj_net::Lobby>,
    chat: Vec<cj_net::ChatLine>,
    settled: Vec<cj_net::Settled>,
    shop: Vec<cj_net::ShopReply>,
    last: Option<Snapshot>,
    /// Every event the server has sent.
    heard: Vec<Emitted>,
    seq: u32,
}

impl Bot {
    fn join(s: &mut Server, id: ClientId, player: u8) -> Bot {
        Bot {
            id,
            client: s.renet.new_local_client(id),
            player: [player; 16],
            pilot: None,
            rejected: None,
            waiting: false,
            info: None,
            lobby: None,
            chat: Vec::new(),
            settled: Vec::new(),
            shop: Vec::new(),
            last: None,
            heard: Vec::new(),
            seq: 0,
        }
    }

    fn send(&mut self, ch: u8, msg: &ClientMsg) {
        self.client.send_message(ch, cj_net::encode(msg));
    }

    fn hello(&mut self, version: &str, secret: u8) {
        let msg = ClientMsg::Hello {
            game_version: version.to_owned(),
            player: self.player,
            secret: [secret; 16],
            pilot: format!("Bot {}", self.id),
            view: (1280, 720),
        };
        self.send(channel::RELIABLE, &msg);
    }

    /// Sends a one-shot action, such as switching PvP.
    fn act(&mut self, action: Action) {
        self.seq += 1;
        let msg = ClientMsg::Action {
            seq: self.seq,
            action,
        };
        self.send(channel::RELIABLE, &msg);
    }

    fn input(&mut self, input: Input) {
        self.seq += 1;
        let msg = ClientMsg::Input {
            seq: self.seq,
            inputs: vec![input],
        };
        self.send(channel::UNRELIABLE, &msg);
    }

    /// Reads everything the server sent.
    fn read(&mut self) {
        for ch in [channel::RELIABLE, channel::UNRELIABLE] {
            while let Some(bytes) = self.client.receive_message(ch) {
                match cj_net::decode::<ServerMsg>(&bytes).expect("server sends valid messages") {
                    ServerMsg::Welcome { pilot, .. } => {
                        self.pilot = Some(pilot);
                        self.waiting = false;
                    }
                    ServerMsg::Waiting => self.waiting = true,
                    ServerMsg::Info(i) => self.info = Some(i),
                    ServerMsg::Lobby(l) => self.lobby = Some(l),
                    ServerMsg::Chat(line) => self.chat.push(line),
                    ServerMsg::Settled(st) => self.settled.push(*st),
                    ServerMsg::Shop(r) => self.shop.push(r),
                    ServerMsg::Reject(r) => self.rejected = Some(r),
                    ServerMsg::Snapshot(s) => self.last = Some(*s),
                    ServerMsg::Events(e) => self.heard.extend(e),
                    ServerMsg::Looks(_) => {}
                }
            }
        }
    }
}

/// One server step with packets moved both ways for every bot.
fn step(s: &mut Server, bots: &mut [&mut Bot]) {
    let dt = Duration::from_secs_f64(STEP);
    for b in bots.iter_mut() {
        b.client.update(dt);
        if s.renet.is_connected(b.id) {
            s.renet
                .process_local_client(b.id, &mut b.client)
                .expect("local client");
        }
    }
    s.update(dt);
    for b in bots.iter_mut() {
        if s.renet.is_connected(b.id) {
            s.renet
                .process_local_client(b.id, &mut b.client)
                .expect("local client");
        }
        b.read();
    }
}

fn steps(s: &mut Server, bots: &mut [&mut Bot], n: u32) {
    for _ in 0..n {
        step(s, bots);
    }
}

/// A client running the same version is welcomed and sent snapshots of
/// its own pilot.
#[test]
fn a_matching_client_is_welcomed_and_sees_its_pilot() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 6);
    assert_eq!(a.pilot, Some(0));
    let snap = a.last.as_ref().expect("a snapshot");
    assert_eq!(snap.you, 0);
    assert_eq!(snap.pilot_count, 1);
}

/// Another version is refused with the server's version, then dropped.
#[test]
fn a_version_mismatch_is_refused_with_a_reason() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello("1.0.0", 7);
    steps(&mut s, &mut [&mut a], 3);
    assert_eq!(
        a.rejected,
        Some(Reject::VersionMismatch {
            server: VERSION.to_owned()
        })
    );
    assert_eq!(a.pilot, None);
    steps(&mut s, &mut [&mut a], 40);
    assert!(!s.renet.is_connected(1), "dropped once the reason was sent");
    assert_eq!(s.player_count(), 0);
}

/// A full shift and a wrong secret are each refused. The same player
/// joining again with the right secret takes over their pilot: the server
/// may still hold the connection of a game that crashed or lost its network,
/// and the old connection is the one told to go.
///
/// Regression: the returning player was refused ("already flying") until
/// the old connection timed out.
#[test]
fn full_shifts_and_wrong_secrets_are_refused_and_a_double_takes_over() {
    let mut s = server(1);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    // The first connection has sent controls, so its message numbers are
    // ahead of a new connection's, which start again at 1.
    for _ in 0..5 {
        a.input(Input::default());
        steps(&mut s, &mut [&mut a], 1);
    }
    let mut full = Bot::join(&mut s, 2, 2);
    full.hello(VERSION, 7);
    let mut impostor = Bot::join(&mut s, 3, 1);
    impostor.hello(VERSION, 8);
    let mut double = Bot::join(&mut s, 4, 1);
    double.hello(VERSION, 7);
    steps(
        &mut s,
        &mut [&mut a, &mut full, &mut impostor, &mut double],
        3,
    );
    assert_eq!(full.rejected, Some(Reject::Full));
    assert_eq!(impostor.rejected, Some(Reject::WrongSecret));
    assert_eq!(double.rejected, None);
    assert_eq!(
        double.pilot,
        Some(0),
        "the same pilot, on the new connection"
    );
    assert_eq!(a.rejected, Some(Reject::AlreadyConnected));
    assert_eq!(s.player_count(), 1);
    // The new connection flies the tug: its first controls (numbered 1,
    // behind the old connection's) are applied, not taken for repeats.
    double.input(Input {
        thrust: 1.0,
        ..Input::default()
    });
    steps(&mut s, &mut [&mut a, &mut double], 4);
    assert_eq!(double.last.as_ref().map(|snap| snap.ack), Some(1));
    assert!(s.world.pilots[0].ship.thrust_level > 0.0, "under thrust");
    // The old connection is dropped once its reason has had time to arrive,
    // and its going doesn't park the tug the new one is flying. (A second
    // of steps, so a lobby list has been sent since.)
    steps(&mut s, &mut [&mut a, &mut double], 60);
    assert!(!s.renet.is_connected(1) && s.renet.is_connected(4));
    assert!(s.world.pilots[0].ship.alive, "still in the field");
    let list = double.lobby.as_ref().expect("a lobby list");
    assert!(!list.players[0].away);
}

/// A lobby player who joins again on a new connection takes over their
/// place too, and the connection they left behind is told to go.
#[test]
fn a_lobby_player_joining_again_takes_over_their_place() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let mut late = Bot::join(&mut s, 2, 2);
    late.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut late], 3);
    assert!(late.waiting);
    // The wrong secret doesn't get the place.
    let mut impostor = Bot::join(&mut s, 3, 2);
    impostor.hello(VERSION, 8);
    let mut again = Bot::join(&mut s, 4, 2);
    again.hello(VERSION, 7);
    steps(
        &mut s,
        &mut [&mut a, &mut late, &mut impostor, &mut again],
        3,
    );
    assert_eq!(impostor.rejected, Some(Reject::WrongSecret));
    assert_eq!(late.rejected, Some(Reject::AlreadyConnected));
    assert!(again.waiting && again.rejected.is_none());
    assert_eq!(s.waiting_count(), 1, "one place, not two");
    // The old connection's going leaves the new one in the lobby.
    steps(&mut s, &mut [&mut a, &mut late, &mut again], 40);
    assert!(!s.renet.is_connected(2));
    assert_eq!(s.waiting_count(), 1);
}

/// The shop marks a career when a request goes through, and the server
/// saves marked careers once a second, and at once when a lobby player
/// leaves (their copy in memory goes with them).
///
/// Regression: every request that went through was saved on the spot,
/// waiting for the disk on the step loop's thread, so a client sending
/// requests at the rate limit could stall the server for everyone.
#[test]
fn shop_changes_are_saved_once_a_second_and_when_a_lobby_player_leaves() {
    use cj_net::ShopReq;
    use cj_sim::field::Crew;
    let dir = std::env::temp_dir().join(format!("cj-session-shop-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Server::new(Config {
        careers: Some(dir.clone()),
        ..config_for(4)
    });
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let mut late = Bot::join(&mut s, 2, 2);
    late.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut late], 3);
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a, &mut late], 3);
    assert!(s.in_break());
    // The co-pilot in player `n`'s career file.
    let saved_crew = |n: u8| {
        let stem: String = [n; 16].iter().map(|b| format!("{b:02x}")).collect();
        let text = std::fs::read_to_string(dir.join(format!("{stem}.json"))).expect("a file");
        cj_sim::career::Career::from_json(&text)
            .expect("a career")
            .crew
    };
    let hire = ClientMsg::Shop(ShopReq::Crew(Crew::Fixer));
    let hired = |bot: &Bot| bot.shop.last().is_some_and(|r| r.ok && !r.msg.is_empty());

    // A flying player hires just after a second's save, so the next save
    // is most of a second away: answered at once, written within a second.
    while s.tick() % 60 != 1 {
        steps(&mut s, &mut [&mut a, &mut late], 1);
    }
    a.send(channel::RELIABLE, &hire);
    steps(&mut s, &mut [&mut a, &mut late], 2);
    assert!(hired(&a));
    assert_eq!(saved_crew(1), None, "hired, but not written on the spot");
    steps(&mut s, &mut [&mut a, &mut late], 60);
    assert_eq!(saved_crew(1), Some(Crew::Fixer), "within a second");

    // A lobby player hires and leaves before the second is up: written as
    // they go.
    while s.tick() % 60 != 1 {
        steps(&mut s, &mut [&mut a, &mut late], 1);
    }
    late.send(channel::RELIABLE, &hire);
    steps(&mut s, &mut [&mut a, &mut late], 2);
    assert!(hired(&late));
    assert_eq!(saved_crew(2), None, "not written yet");
    s.renet.disconnect_local_client(2, &mut late.client);
    steps(&mut s, &mut [&mut a], 2);
    assert_eq!(s.waiting_count(), 0, "gone from the lobby");
    assert_eq!(saved_crew(2), Some(Crew::Fixer), "written as they left");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A player who leaves takes their tug and its cargo out of the field, so
/// nothing can hurt or rob it, and both come back as they were on return.
#[test]
fn an_away_players_tug_and_cargo_leave_the_field_until_they_return() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    // Pilot 1 tows one piece of scrap 40 units behind its tug, well away
    // from the bay (where towed cargo would be banked).
    s.world.pilots[1].ship.b.x += 1500.0;
    s.world.pilots[1].ship.b.y += 1500.0;
    let tug = s.world.pilots[1].ship.b;
    let mut it = s.world.new_item(
        cj_sim::items::ItemType::Scrap,
        tug.x - 40.0,
        tug.y,
        0.0,
        0.0,
    );
    it.towed = true;
    it.link_len = 40.0;
    it.owner = Some(1);
    let id = it.id;
    s.world.items.push(it);
    s.world.pilots[1].chain.push(id);

    s.renet.disconnect_local_client(2, &mut b.client);
    steps(&mut s, &mut [&mut a], 3);
    let parked = s.world.pilots[1].ship.b;
    assert!(!s.world.pilots[1].ship.alive, "out of the field");
    assert!(s.world.items.iter().all(|it| it.id != id), "cargo too");
    let snap = a.last.as_ref().expect("a snapshot");
    assert!(
        snap.others.iter().all(|o| o.slot != 1 || !o.ship.alive),
        "the others don't see it"
    );
    steps(&mut s, &mut [&mut a], 120);
    assert_eq!(s.world.pilots[1].ship.b, parked, "frozen while away");

    let mut back = Bot::join(&mut s, 3, 2);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut back], 3);
    assert_eq!(back.pilot, Some(1));
    assert!(s.world.pilots[1].ship.alive, "back in the field");
    assert_eq!(s.world.pilots[1].chain, vec![id], "still towing it");
    assert!(s.world.items.iter().any(|it| it.id == id));
}

/// Ends the shift on the next step.
fn end_shift(s: &mut Server) {
    s.world.time_left = 0.001;
}

/// In the shift's last minute a newcomer waits in the lobby while a player
/// who already has a pilot comes straight back to it; when everyone is
/// ready the next shift starts with both flying.
#[test]
fn a_late_newcomer_waits_in_the_lobby_for_the_next_shift() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    s.renet.disconnect_local_client(1, &mut a.client);
    steps(&mut s, &mut [], 3);
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let mut late = Bot::join(&mut s, 2, 2);
    late.hello(VERSION, 7);
    let mut back = Bot::join(&mut s, 3, 1);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut late, &mut back], 3);
    assert!(late.waiting && late.pilot.is_none(), "in the lobby");
    assert_eq!(
        back.pilot,
        Some(0),
        "a returning player reclaims their pilot"
    );
    assert_eq!(s.world.pilots.len(), 1, "no new tug yet");
    assert_eq!(s.waiting_count(), 1);
    steps(&mut s, &mut [&mut late, &mut back], 60);
    let lobby = late.lobby.as_ref().expect("the lobby list");
    assert_eq!(lobby.players.len(), 2);
    assert_eq!(lobby.players[1].slot, None, "listed as waiting");

    end_shift(&mut s);
    steps(&mut s, &mut [&mut late, &mut back], 3);
    assert!(s.in_break());
    for bot in [&mut late, &mut back] {
        bot.send(channel::RELIABLE, &ClientMsg::Ready);
    }
    steps(&mut s, &mut [&mut late, &mut back], 3);
    assert!(!s.in_break(), "everyone was ready");
    assert_eq!(s.shift_no(), 2);
    assert_eq!((back.pilot, late.pilot), (Some(0), Some(1)));
    assert!(!late.waiting);
    assert_eq!(s.world.pilots.len(), 2);
    steps(&mut s, &mut [&mut late, &mut back], 6);
    assert_eq!(late.last.as_ref().expect("a snapshot").you, 1);
}

/// Regression: a newcomer who arrived during a break was told to wait but
/// sent no career, so their shop stayed shut until another shift had been
/// flown. They get it on arrival, with the break's offers ready.
#[test]
fn a_newcomer_during_a_break_gets_their_career_to_shop_with() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a], 3);
    assert!(s.in_break());
    let mut late = Bot::join(&mut s, 2, 2);
    late.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut late], 3);
    assert!(late.waiting && late.pilot.is_none(), "in the lobby");
    let reply = late.shop.last().expect("their career, unasked");
    assert!(reply.ok);
    let career = cj_sim::career::Career::from_json(&reply.career).expect("a career");
    assert!(
        career.conditions.is_some(),
        "the break's conditions are set"
    );
}

/// At the end of a shift each player's own haul is settled into their own
/// career and sent to them, and the career is saved: a server started
/// again on the same folder gives the returning player the same career.
#[test]
fn each_career_is_settled_saved_and_kept_across_a_restart() {
    let dir = std::env::temp_dir().join(format!("cj-session-careers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = |seed| Config {
        version: VERSION.to_owned(),
        max_players: 4,
        seed,
        record: false,
        mode: cj_sim::career::Mode::Hard,
        careers: Some(dir.clone()),
        pvp: false,
    };
    let mut s = Server::new(config(42));
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert_eq!(
        s.world.pilots[0].career.as_ref().map(|c| c.mode),
        Some(cj_sim::career::Mode::Hard),
        "careers in the server's mode"
    );
    s.world.pilots[1].banked = 1500.0;
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert_eq!((a.settled.len(), b.settled.len()), (1, 1));
    assert_eq!(a.settled[0].settlement.earned, 0.0);
    assert!(
        b.settled[0].settlement.earned > 0.0,
        "the second pilot's haul is theirs"
    );
    let after = cj_sim::career::Career::from_json(&b.settled[0].career).expect("a career");
    assert_eq!(after.shift, 2);

    drop(s);
    let mut again = Server::new(config(43));
    let mut back = Bot::join(&mut again, 1, 2);
    back.hello(VERSION, 7);
    steps(&mut again, &mut [&mut back], 3);
    let kept = again.world.pilots[0].career.as_ref().expect("a career");
    assert_eq!(
        (kept.shift, kept.wallet),
        (after.shift, after.wallet),
        "the same career"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression: a saved career was loaded by player id alone, so after a
/// restart (or once the server had forgotten a player who left) any secret
/// claimed it. Now only the secret that first claimed it may.
#[test]
fn a_saved_career_refuses_the_wrong_secret_after_a_restart() {
    let dir = std::env::temp_dir().join(format!("cj-session-secret-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = |seed| Config {
        careers: Some(dir.clone()),
        seed,
        ..config_for(4)
    };
    let mut s = Server::new(config(42));
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    s.world.pilots[0].banked = 1500.0;
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a], 3);
    let saved = cj_sim::career::Career::from_json(&a.settled[0].career).expect("a career");

    drop(s);
    let mut again = Server::new(config(43));
    let mut thief = Bot::join(&mut again, 1, 1);
    thief.hello(VERSION, 8);
    steps(&mut again, &mut [&mut thief], 3);
    assert_eq!(thief.rejected, Some(Reject::WrongSecret));
    assert!(thief.pilot.is_none() && !thief.waiting);
    assert!(
        again.world.pilots[0].career.is_none(),
        "nobody got the career"
    );

    let mut owner = Bot::join(&mut again, 2, 1);
    owner.hello(VERSION, 7);
    steps(&mut again, &mut [&mut owner], 3);
    assert_eq!(owner.pilot, Some(0));
    let kept = again.world.pilots[0].career.as_ref().expect("a career");
    assert_eq!((kept.shift, kept.wallet), (saved.shift, saved.wallet));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression: when the key binding a career to its owner's secret couldn't
/// be saved, the server still let the player in and saved their career,
/// with no key, for the next secret to claim. Now it turns them away with a
/// reason of its own and writes no career.
#[test]
fn a_key_that_cant_be_saved_turns_the_player_away() {
    let dir = std::env::temp_dir().join(format!("cj-session-nokey-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stem: String = [1u8; 16].iter().map(|b| format!("{b:02x}")).collect();
    // A folder where the key's temporary file goes: it can't be created.
    std::fs::create_dir_all(dir.join(format!("{stem}.key.tmp"))).expect("made");
    let mut s = Server::new(Config {
        careers: Some(dir.clone()),
        ..config_for(4)
    });
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    assert_eq!(a.rejected, Some(Reject::ServerError));
    assert!(a.pilot.is_none() && !a.waiting);
    assert!(
        !dir.join(format!("{stem}.json")).exists(),
        "no career without its key"
    );
    assert!(!dir.join(format!("{stem}.key")).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression: every newcomer wrote a key and a career to disk, and one who
/// waited in the lobby freed the slot by leaving, so a script repeating that
/// with fresh ids filled the careers folder without end. Now a server lets
/// in at most `max_players` newcomers a minute: past that a new id is
/// refused as Busy with nothing written, a known player still gets in, and
/// newcomers are welcome again the next minute.
#[test]
fn newcomers_past_the_minutes_cap_are_refused_and_write_nothing() {
    let dir = std::env::temp_dir().join(format!("cj-session-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Server::new(Config {
        careers: Some(dir.clone()),
        ..config_for(2)
    });
    // In the shift's last minute newcomers wait in the lobby, and leaving it
    // frees the slot at once: the route a flood would take.
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let stem = |p: u8| -> String { [p; 16].iter().map(|b| format!("{b:02x}")).collect() };
    for (conn, player) in [(1, 1), (2, 2)] {
        let mut b = Bot::join(&mut s, conn, player);
        b.hello(VERSION, 7);
        steps(&mut s, &mut [&mut b], 3);
        assert!(b.waiting, "newcomer {player} is let in");
        s.renet.disconnect_local_client(conn, &mut b.client);
        steps(&mut s, &mut [], 1);
    }
    assert_eq!(s.waiting_count(), 0, "the lobby is empty again");

    let mut third = Bot::join(&mut s, 3, 3);
    third.hello(VERSION, 7);
    steps(&mut s, &mut [&mut third], 3);
    assert_eq!(third.rejected, Some(Reject::Busy));
    assert!(!dir.join(format!("{}.key", stem(3))).exists(), "no key");
    assert!(!dir.join(format!("{}.json", stem(3))).exists(), "no career");

    let mut back = Bot::join(&mut s, 4, 1);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut back], 3);
    assert!(back.waiting, "a player with a key here still gets in");

    // A minute later newcomers are welcome again.
    steps(&mut s, &mut [&mut back], 60 * 60);
    let mut later = Bot::join(&mut s, 5, 3);
    later.hello(VERSION, 7);
    steps(&mut s, &mut [&mut back, &mut later], 3);
    assert!(later.waiting && later.rejected.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression: a nameless pilot (one whose name is only in scripts the
/// fonts can't draw, so nothing is left once it's cleaned) became "Pilot N"
/// when joining mid-shift but "New pilot" in the lobby, so two of them in
/// the lobby looked the same in its list and the chat. Now each nameless
/// player gets a "Pilot N" nobody else on the server has.
#[test]
fn nameless_pilots_get_names_nobody_else_has() {
    let mut s = server(4);
    let nameless_hello = |b: &mut Bot| {
        let msg = ClientMsg::Hello {
            game_version: VERSION.to_owned(),
            player: b.player,
            secret: [7; 16],
            pilot: "Пётр".to_owned(),
            view: (1280, 720),
        };
        b.send(channel::RELIABLE, &msg);
    };
    let mut a = Bot::join(&mut s, 1, 1);
    nameless_hello(&mut a);
    steps(&mut s, &mut [&mut a], 3);
    assert_eq!(a.pilot, Some(0), "flying");
    // The shift's last minute: the next two wait in the lobby.
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let mut b = Bot::join(&mut s, 2, 2);
    nameless_hello(&mut b);
    let mut c = Bot::join(&mut s, 3, 3);
    nameless_hello(&mut c);
    // A lobby list goes out every second.
    steps(&mut s, &mut [&mut a, &mut b, &mut c], 61);
    assert!(b.waiting && c.waiting);
    let names: Vec<String> = a
        .lobby
        .as_ref()
        .expect("the lobby list")
        .players
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names, ["Pilot 1", "Pilot 2", "Pilot 3"]);
}

/// Regression: a player who reconnected during the break got their pilot
/// back but not their career, so their shop and belt vote stayed shut until
/// another shift had been flown.
#[test]
fn a_player_back_during_the_break_gets_their_career_to_shop_with() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert!(s.in_break());
    s.renet.disconnect_local_client(2, &mut b.client);
    steps(&mut s, &mut [&mut a], 3);
    let mut back = Bot::join(&mut s, 3, 2);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut back], 3);
    assert_eq!(back.pilot, Some(1), "the same pilot as before");
    let reply = back.shop.last().expect("their career, unasked");
    assert!(reply.ok);
    let career = cj_sim::career::Career::from_json(&reply.career).expect("a career");
    assert!(
        career.conditions.is_some(),
        "the break's conditions are set"
    );
}

/// Regression: a lobby player's offers were rolled at the break but not
/// saved, so leaving and joining again reloaded the career without them and
/// rolled a fresh set, as often as the player liked. Both lobby paths (waiting
/// from the shift's last minute, and arriving during the break) now save the
/// roll, and a player who comes back sees the same offers.
#[test]
fn lobby_offers_survive_leaving_and_coming_back_in_the_break() {
    let dir = std::env::temp_dir().join(format!("cj-session-offers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Server::new(Config {
        careers: Some(dir.clone()),
        ..config_for(4)
    });
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    // Waits from the shift's last minute: its offers are rolled at the break.
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let mut late = Bot::join(&mut s, 2, 2);
    late.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut late], 3);
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a, &mut late], 3);
    assert!(s.in_break());
    // Arrives during the break: its offers are rolled on arrival.
    let mut early = Bot::join(&mut s, 3, 3);
    early.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut late, &mut early], 3);

    // The offers each lobby player was last sent, from their career.
    let offers = |b: &Bot| {
        let reply = b.shop.last().expect("their career, unasked");
        cj_sim::career::Career::from_json(&reply.career)
            .expect("a career")
            .offers
    };
    let first = (offers(&late), offers(&early));
    assert!(!first.0.is_empty() && !first.1.is_empty(), "offers rolled");

    // Both leave the lobby and join again on new connections.
    s.renet.disconnect_local_client(2, &mut late.client);
    s.renet.disconnect_local_client(3, &mut early.client);
    steps(&mut s, &mut [&mut a], 3);
    assert_eq!(s.waiting_count(), 0, "the lobby is empty");
    let mut late_again = Bot::join(&mut s, 4, 2);
    late_again.hello(VERSION, 7);
    let mut early_again = Bot::join(&mut s, 5, 3);
    early_again.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut late_again, &mut early_again], 3);
    assert_eq!(
        (offers(&late_again), offers(&early_again)),
        first,
        "the same offers, not a new roll"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The shop opens only in the break and checks each request against the
/// player's career; belt votes pick the next shift's belt among those
/// everyone's tug can fly, with its conditions rolled for the break.
#[test]
fn the_break_shop_checks_requests_and_votes_pick_the_belt() {
    use cj_net::ShopReq;
    use cj_sim::field::Belt;
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    a.send(
        channel::RELIABLE,
        &ClientMsg::Shop(ShopReq::Vote(Belt::Hollis)),
    );
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert!(!a.shop[0].ok, "shut during a shift");

    end_shift(&mut s);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert!(s.in_break());
    let shop = |bot: &mut Bot, req| bot.send(channel::RELIABLE, &ClientMsg::Shop(req));
    shop(&mut a, ShopReq::Offer(99));
    shop(&mut a, ShopReq::Vote(Belt::Frontier));
    shop(&mut a, ShopReq::Vote(Belt::Hollis));
    shop(&mut b, ShopReq::Vote(Belt::Hollis));
    steps(&mut s, &mut [&mut a, &mut b], 3);
    let said: Vec<bool> = a.shop[1..].iter().map(|r| r.ok).collect();
    assert_eq!(
        said,
        vec![false, false, true],
        "no such offer; no permit; a vote"
    );
    let career = cj_sim::career::Career::from_json(&a.shop[3].career).expect("a career");
    assert_eq!(career.belt, Belt::Hollis);
    steps(&mut s, &mut [&mut a, &mut b], 60);
    let votes = &a.lobby.as_ref().expect("a list").votes;
    assert_eq!(votes, &vec![(Belt::Hollis, 2)]);

    for bot in [&mut a, &mut b] {
        bot.send(channel::RELIABLE, &ClientMsg::Ready);
    }
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert!(!s.in_break());
    assert_eq!(s.world.belt, Belt::Hollis, "the vote won");
    let rolled = career.conditions.expect("rolled for the break").1;
    assert_eq!(s.world.cond, rolled[1], "Hollis's condition as rolled");
}

/// Online the server is the only check on what a career buys: a part the
/// wallet can't cover, and a chain-reward part (priced at 0 because it is
/// never sold), are both refused whatever the client sends.
#[test]
fn the_break_shop_refuses_what_the_career_cannot_buy() {
    use cj_net::ShopReq;
    use cj_sim::parts::Part;
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    // A haul that leaves 1,000 cr after the first installment: enough for a
    // free part, so only the chain-reward rule can refuse it, but not armor.
    let due = cj_sim::career::Career::new(cj_sim::career::Mode::Standard).due(0);
    s.world.pilots[0].banked = due + 1000.0;
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a], 3);
    assert!(s.in_break());
    let settled = cj_sim::career::Career::from_json(&a.settled[0].career).expect("a career");
    assert_eq!(settled.wallet, 1000.0);
    assert_eq!(
        Part::HullSeal.def().price,
        0.0,
        "the wallet alone can't refuse it"
    );
    for req in [ShopReq::Buy(Part::Armor), ShopReq::Buy(Part::HullSeal)] {
        a.send(channel::RELIABLE, &ClientMsg::Shop(req));
    }
    steps(&mut s, &mut [&mut a], 3);
    assert_eq!(a.shop.len(), 2);
    for reply in &a.shop {
        assert!(!reply.ok, "should be refused, got: {}", reply.msg);
        let after = cj_sim::career::Career::from_json(&reply.career).expect("a career");
        assert_eq!(after.wallet, settled.wallet, "nothing was charged");
        assert!(!after.owns(Part::Armor) && !after.owns(Part::HullSeal));
    }
}

/// The break ends after 90 s even if nobody says they are ready, and a
/// player who left before it gives up their slot.
#[test]
fn the_break_ends_on_time_and_frees_the_slots_of_players_who_left() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    s.renet.disconnect_local_client(2, &mut b.client);
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a], 3);
    assert!(s.in_break());
    assert_eq!(s.player_count(), 2, "kept until the next shift");
    steps(&mut s, &mut [&mut a], cj_server::BREAK_STEPS - 10);
    assert!(s.in_break(), "still waiting for the ready");
    steps(&mut s, &mut [&mut a], 20);
    assert!(!s.in_break(), "the break has a cap");
    assert_eq!(s.player_count(), 1, "the leaver's slot is free");
    assert_eq!(s.world.pilots.len(), 1);
    assert_eq!(a.pilot, Some(0));
}

/// A chat line reaches everyone connected, flying or in the lobby, under
/// the sender's name; it is cleaned, and a flood is cut to the rate limit.
#[test]
fn chat_reaches_everyone_cleaned_and_rate_limited() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    s.world.time_left = cj_server::LATE_JOIN_CUTOFF - 1.0;
    let mut late = Bot::join(&mut s, 3, 3);
    late.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b, &mut late], 3);
    assert!(late.waiting);

    a.send(
        channel::RELIABLE,
        &ClientMsg::Chat {
            text: "  hello\n\tall ".to_owned(),
        },
    );
    late.send(
        channel::RELIABLE,
        &ClientMsg::Chat {
            text: "room for one more?".to_owned(),
        },
    );
    steps(&mut s, &mut [&mut a, &mut b, &mut late], 3);
    for bot in [&a, &b, &late] {
        assert_eq!(bot.chat.len(), 2, "everyone hears both");
        assert_eq!(
            (
                bot.chat[0].from.as_str(),
                bot.chat[0].slot,
                bot.chat[0].text.as_str()
            ),
            ("Bot 1", Some(0), "hello all")
        );
        assert_eq!(bot.chat[1].slot, None, "from the lobby");
    }

    for i in 0..10 {
        b.send(
            channel::RELIABLE,
            &ClientMsg::Chat {
                text: format!("spam {i}"),
            },
        );
    }
    steps(&mut s, &mut [&mut a, &mut b, &mut late], 3);
    assert_eq!(a.chat.len(), 4, "two a second get through");
}

/// Before Hello a client can ask how full the server is, and asking keeps
/// its connection open past the Hello timeout.
#[test]
fn a_query_reports_how_full_the_server_is() {
    let mut s = server(1);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    let mut q = Bot::join(&mut s, 2, 2);
    for _ in 0..8 {
        q.send(channel::RELIABLE, &ClientMsg::Query);
        steps(&mut s, &mut [&mut a, &mut q], 60);
    }
    assert!(s.renet.is_connected(2), "still connected after 8 s");
    let info = q.info.as_ref().expect("an answer");
    assert_eq!(info.version, VERSION);
    assert_eq!((info.connected, info.max, info.full), (1, 1, true));
}

/// A pilot's sounds reach players whose tugs are within hearing, but not
/// those beyond it, and the rest of a pilot's own feedback (here its
/// engine) reaches no one else.
#[test]
fn nearby_players_hear_each_other_and_far_ones_do_not() {
    let mut s = server(3);
    let mut bots: Vec<Bot> = (1..=3).map(|i| Bot::join(&mut s, i, i as u8)).collect();
    for b in &mut bots {
        b.hello(VERSION, 7);
    }
    steps(&mut s, &mut bots.iter_mut().collect::<Vec<_>>(), 3);
    let (x, y) = (s.world.pilots[0].ship.b.x, s.world.pilots[0].ship.b.y);
    s.world.pilots[1].ship.b.x = x + cj_net::HEARING / 3.0;
    s.world.pilots[1].ship.b.y = y;
    s.world.pilots[2].ship.b.x = x + cj_net::HEARING * 2.0;
    s.world.pilots[2].ship.b.y = y;
    for b in &mut bots {
        b.heard.clear();
    }
    s.world.events.push(Emitted {
        to: Some(0),
        event: Event::Sfx(Sfx::Shoot(1)),
    });
    s.world.events.push(Emitted {
        to: Some(0),
        event: Event::Thrust {
            level: 1.0,
            load: 0.0,
            sputtering: false,
        },
    });
    steps(&mut s, &mut bots.iter_mut().collect::<Vec<_>>(), 3);
    let shot = |b: &Bot| {
        b.heard
            .iter()
            .any(|e| e.to == Some(0) && e.event == Event::Sfx(Sfx::Shoot(1)))
    };
    let engine = |b: &Bot| {
        b.heard
            .iter()
            .any(|e| e.to == Some(0) && matches!(e.event, Event::Thrust { .. }))
    };
    assert!(shot(&bots[0]) && engine(&bots[0]), "the shooter hears both");
    assert!(shot(&bots[1]), "a nearby player hears the shot");
    assert!(!engine(&bots[1]), "but not the shooter's engine");
    assert!(!shot(&bots[2]), "a far player hears nothing");
}

/// Three players join, one leaves while the others keep flying, and on
/// return reclaims the same pilot; a fourth player then gets a fresh one.
#[test]
fn three_players_join_leave_and_rejoin() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    let mut b = Bot::join(&mut s, 2, 2);
    let mut c = Bot::join(&mut s, 3, 3);
    for bot in [&mut a, &mut b, &mut c] {
        bot.hello(VERSION, 7);
    }
    steps(&mut s, &mut [&mut a, &mut b, &mut c], 3);
    assert_eq!((a.pilot, b.pilot, c.pilot), (Some(0), Some(1), Some(2)));
    for (bot, you) in [(&a, 0), (&b, 1), (&c, 2)] {
        let snap = bot.last.as_ref().expect("a snapshot");
        assert_eq!((snap.you, snap.pilot_count), (you, 3));
    }

    s.renet.disconnect_local_client(2, &mut b.client);
    let before = a.last.as_ref().expect("a snapshot").tick;
    steps(&mut s, &mut [&mut a, &mut c], 6);
    assert_eq!(s.renet.connected_clients(), 2);
    assert_eq!(s.player_count(), 3, "the leaver's pilot is kept for them");
    for bot in [&a, &c] {
        assert!(
            bot.last.as_ref().expect("a snapshot").tick > before,
            "still flying"
        );
    }

    let mut back = Bot::join(&mut s, 4, 2);
    back.hello(VERSION, 7);
    let mut d = Bot::join(&mut s, 5, 4);
    d.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut c, &mut back, &mut d], 3);
    assert_eq!(back.pilot, Some(1), "the same pilot as before");
    assert_eq!(d.pilot, Some(3), "a newcomer gets a new pilot");
    assert_eq!(s.world.pilots.len(), 4);
    assert_eq!(s.renet.connected_clients(), 4);
}

/// A player who leaves keeps their pilot and reclaims it on return.
#[test]
fn a_returning_player_reclaims_their_pilot() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert_eq!(b.pilot, Some(1));
    // Two seconds of flying numbers b's controls up to 120.
    for _ in 0..120 {
        b.input(Input::default());
        steps(&mut s, &mut [&mut a, &mut b], 1);
    }
    s.renet.disconnect_local_client(2, &mut b.client);
    steps(&mut s, &mut [&mut a], 3);
    // The returning game is a new connection, numbering from 1 again.
    let mut back = Bot::join(&mut s, 3, 2);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut back], 3);
    assert_eq!(back.pilot, Some(1), "the same pilot as before");
    assert_eq!(s.world.pilots.len(), 2, "no new tug");
    let thrust = Input {
        thrust: 1.0,
        ..Input::default()
    };
    for _ in 0..30 {
        back.input(thrust);
        steps(&mut s, &mut [&mut a, &mut back], 1);
    }
    let ack = back.last.as_ref().expect("a snapshot").ack;
    assert!(
        (25..=30).contains(&ack),
        "the new connection's controls are applied, not dropped as old (ack {ack})"
    );
    assert!(s.world.pilots[1].ship.throttle > 0.0, "the tug answers");
}

/// Garbage gets a client disconnected; non-finite controls are refused.
#[test]
fn hostile_messages_are_refused() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    a.input(Input {
        thrust: f64::NAN,
        ..Input::default()
    });
    steps(&mut s, &mut [&mut a], 3);
    assert_eq!(
        s.world.pilots[0].ship.thrust_level, 0.0,
        "the NaN never flew"
    );
    for _ in 0..12 {
        a.client
            .send_message(channel::RELIABLE, vec![0xffu8, 0x13, 0x37]);
    }
    steps(&mut s, &mut [&mut a], 3);
    assert!(
        !s.renet.is_connected(1),
        "dropped after too many invalid messages"
    );
}

/// A flood of controls in one second is cut off at the rate limit.
#[test]
fn a_flood_of_inputs_is_rate_limited() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 1);
    for _ in 0..1000 {
        a.input(Input {
            thrust: 1.0,
            ..Input::default()
        });
    }
    steps(&mut s, &mut [&mut a], 3);
    let ack = a.last.as_ref().expect("a snapshot").ack;
    assert!(ack > 0 && ack <= 180, "applied up to the limit, got {ack}");
    assert!(
        s.renet.is_connected(1),
        "flooding drops messages, not the client"
    );
}

/// Two pilots fly their own tugs, and each sees only its own surroundings.
#[test]
fn two_pilots_fly_their_own_tugs_and_see_their_own_view() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    assert_eq!(
        (a.pilot, b.pilot),
        (Some(0), Some(1)),
        "slots go in connection order"
    );
    let start = (s.world.pilots[0].ship.b, s.world.pilots[1].ship.b);
    for _ in 0..60 {
        b.input(Input {
            thrust: 1.0,
            ..Input::default()
        });
        step(&mut s, &mut [&mut a, &mut b]);
    }
    let (a0, b0) = start;
    let (a1, b1) = (s.world.pilots[0].ship.b, s.world.pilots[1].ship.b);
    assert!(
        cj_sim::js::dist(a0.x, a0.y, a1.x, a1.y) < 5.0,
        "pilot 0 stayed parked"
    );
    assert!(
        cj_sim::js::dist(b0.x, b0.y, b1.x, b1.y) > 50.0,
        "pilot 1 flew"
    );

    // A rare piece in a far corner is only a radar dot for both.
    let mut far = s
        .world
        .new_item(cj_sim::items::ItemType::Scrap, 100.0, 100.0, 0.0, 0.0);
    far.rare = true;
    let id = far.id;
    s.world.items.push(far);
    steps(&mut s, &mut [&mut a, &mut b], 3);
    for bot in [&a, &b] {
        let snap = bot.last.as_ref().expect("a snapshot");
        assert!(snap.items.iter().all(|it| it.id != id), "not sent in full");
        assert!(
            snap.blips.iter().any(|d| (d.x, d.y) == (100, 100)),
            "but on the radar"
        );
    }
    assert_eq!(b.last.as_ref().expect("a snapshot").you, 1);
}

/// Replaying the recording reproduces the shift exactly.
#[test]
fn a_recording_replays_the_shift_exactly() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    for t in 0..600u32 {
        let wobble = (f64::from(t) * STEP * 2.0).sin();
        a.input(Input {
            thrust: 0.8,
            turn: wobble,
            fire: t % 40 < 20,
            ..Input::default()
        });
        b.input(Input {
            thrust: 0.5,
            turn: -wobble,
            ..Input::default()
        });
        if t % 90 == 30 {
            a.seq += 1;
            let seq = a.seq;
            a.send(
                channel::RELIABLE,
                &ClientMsg::Action {
                    seq,
                    action: Action::Hook,
                },
            );
        }
        step(&mut s, &mut [&mut a, &mut b]);
    }
    let rec = s.recording().expect("recording").clone();
    let w = replay(&rec, s.tick());
    for p in 0..2 {
        let (live, again) = (&s.world.pilots[p].ship, &w.pilots[p].ship);
        assert_eq!(
            (live.b, live.angle, live.hp),
            (again.b, again.angle, again.hp),
            "pilot {p}"
        );
    }
    assert_eq!(s.world.items.len(), w.items.len());
    assert_eq!(s.world.jumpers.len(), w.jumpers.len());
}

/// A three-player shift replays exactly too, with one player joining late
/// and another leaving partway through.
#[test]
fn a_three_player_recording_with_a_late_join_and_a_leaver_replays_exactly() {
    let mut s = server(4);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    let mut b = Bot::join(&mut s, 2, 2);
    b.hello(VERSION, 7);
    // The late player connects 150 steps in (a connection that says
    // nothing would be dropped before then).
    let mut c: Option<Bot> = None;
    let fly = |t: u32, k: f64| Input {
        thrust: 0.7,
        turn: (f64::from(t) * STEP * k).sin(),
        fire: t % 50 < 10,
        ..Input::default()
    };
    for t in 0..600u32 {
        if t == 150 {
            let mut late = Bot::join(&mut s, 3, 3);
            late.hello(VERSION, 7);
            c = Some(late);
        }
        if t == 400 {
            s.renet.disconnect_local_client(2, &mut b.client);
        }
        a.input(fly(t, 2.0));
        let mut bots: Vec<&mut Bot> = vec![&mut a];
        if t < 400 {
            b.input(fly(t, 1.0));
            bots.push(&mut b);
        }
        if let Some(late) = c.as_mut() {
            late.input(fly(t, -3.0));
            bots.push(late);
        }
        step(&mut s, &mut bots);
    }
    assert_eq!(c.and_then(|late| late.pilot), Some(2));
    assert_eq!(s.world.pilots.len(), 3);
    let rec = s.recording().expect("recording").clone();
    let w = replay(&rec, s.tick());
    for p in 0..3 {
        let (live, again) = (&s.world.pilots[p].ship, &w.pilots[p].ship);
        assert_eq!(
            (live.b, live.angle, live.hp),
            (again.b, again.angle, again.hp),
            "pilot {p}"
        );
    }
    assert_eq!(s.world.items.len(), w.items.len());
    assert_eq!(s.world.jumpers.len(), w.jumpers.len());
    assert_eq!(s.world.bullets.len(), w.bullets.len());
}

/// A server for 20 pilots sizes its field for them (10.5 times the area,
/// around the carrier), and a client sees a far corner on its radar with
/// the field's real, negative coordinates.
#[test]
fn a_twenty_pilot_server_sizes_its_field_and_reports_far_corners() {
    let mut s = Server::new(Config {
        version: VERSION.to_owned(),
        max_players: 20,
        seed: 42,
        record: false,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
        pvp: false,
    });
    let w = &s.world;
    let width = w.edge_hi - w.edge_lo;
    assert!((width / cj_sim::consts::WORLD_W - 10.5f64.sqrt()).abs() < 1e-9);
    assert!(w.edge_lo < 0.0);
    let corner = (w.edge_lo + 150.0, w.edge_lo + 150.0);
    let mut far = s
        .world
        .new_item(cj_sim::items::ItemType::Scrap, corner.0, corner.1, 0.0, 0.0);
    far.rare = true;
    s.world.items.push(far);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 6);
    let snap = a.last.as_ref().expect("a snapshot");
    assert_eq!(
        (snap.edge_lo, snap.edge_hi),
        (s.world.edge_lo, s.world.edge_hi)
    );
    let (cx, cy) = (corner.0.round() as i32, corner.1.round() as i32);
    assert!(
        snap.blips
            .iter()
            .any(|d| (d.x - cx).abs() <= 1 && (d.y - cy).abs() <= 1),
        "the far corner's piece is on the radar"
    );
}

/// A host's PvP choice reaches every shift's field and the Join screen's
/// answer; without it a server is co-op only.
#[test]
fn the_hosts_pvp_choice_holds_for_every_shift() {
    let mut s = Server::new(Config {
        pvp: true,
        ..config_for(4)
    });
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
    assert!(s.world.pvp_allowed);
    let mut q = Bot::join(&mut s, 2, 2);
    q.send(channel::RELIABLE, &ClientMsg::Query);
    steps(&mut s, &mut [&mut a, &mut q], 3);
    assert!(q.info.as_ref().expect("an answer").pvp);
    end_shift(&mut s);
    steps(&mut s, &mut [&mut a, &mut q], cj_server::BREAK_STEPS + 20);
    assert!(!s.in_break(), "the next shift is on");
    assert!(s.world.pvp_allowed, "still allowed after the break");
    assert!(!server(4).world.pvp_allowed, "off by default");
}

/// Steps for `secs` seconds of shift time, plus a few to settle.
fn seconds(s: &mut Server, bots: &mut [&mut Bot], secs: f64) {
    steps(s, bots, (secs / STEP).ceil() as u32 + 5);
}

/// Three players on a PvP server, each tug out in the open with no shield
/// or spawn cover, a lane apart so stray rounds can't cross.
fn pvp_trio() -> (Server, Bot, Bot, Bot) {
    let mut s = Server::new(Config {
        pvp: true,
        ..config_for(4)
    });
    let mut a = Bot::join(&mut s, 1, 1);
    let mut b = Bot::join(&mut s, 2, 2);
    let mut c = Bot::join(&mut s, 3, 3);
    for bot in [&mut a, &mut b, &mut c] {
        bot.hello(VERSION, 7);
    }
    steps(&mut s, &mut [&mut a, &mut b, &mut c], 3);
    assert_eq!((a.pilot, b.pilot, c.pilot), (Some(0), Some(1), Some(2)));
    (s, a, b, c)
}

/// Pilot 0 asks to go PvP from the bay (where every tug starts) and waits
/// out the arming time.
fn arm_first(s: &mut Server, a: &mut Bot, b: &mut Bot, c: &mut Bot) {
    a.act(Action::TogglePvp);
    steps(s, &mut [&mut *a, &mut *b, &mut *c], 2);
    assert!(matches!(s.world.pilots[0].pvp, Pvp::Arming { .. }));
    seconds(s, &mut [a, b, c], cj_sim::perm::PVP_ARM_TIME);
    assert_eq!(s.world.pilots[0].pvp, Pvp::Enabled);
}

/// Moves every tug out of the bay into its own lane, uncovered.
fn spread_out(s: &mut Server) {
    for p in 0..s.world.pilots.len() {
        let sh = &mut s.world.pilots[p].ship;
        sh.b.x = cj_sim::consts::BAY_X + 1500.0;
        sh.b.y = cj_sim::consts::BAY_Y + 300.0 * p as f64;
        (sh.b.vx, sh.b.vy) = (0.0, 0.0);
        (sh.invuln, sh.shield) = (0.0, 0.0);
    }
}

/// Fires one cutter round for pilot `by` from just beside pilot `at`'s tug,
/// steps until it lands, and returns the shield and hull `at` lost (the
/// shield regrows, so it may take the whole round).
fn shoot(s: &mut Server, bots: &mut [&mut Bot], by: usize, at: usize) -> f64 {
    let (x, y) = (s.world.pilots[at].ship.b.x, s.world.pilots[at].ship.b.y);
    let cover = |s: &Server| s.world.pilots[at].ship.hp + s.world.pilots[at].ship.shield;
    let before = cover(s);
    s.world.bullets.push(cj_sim::entities::Bullet {
        x: x - 40.0,
        y,
        vx: 600.0,
        life: 1.0,
        dmg: 5.0,
        blame: cj_sim::world::Blame::Pilot(by),
        ..cj_sim::entities::Bullet::default()
    });
    steps(s, bots, 8);
    assert!(s.world.bullets.is_empty(), "the round landed");
    before - cover(s)
}

/// Over the network: a player arms PvP from the bay and every client sees
/// the mark; the PvP tug can hurt a Protected one, which may fire back for
/// a while without a star, while a bystander can hurt neither.
#[test]
fn pvp_rules_hold_across_three_players() {
    let (mut s, mut a, mut b, mut c) = pvp_trio();
    arm_first(&mut s, &mut a, &mut b, &mut c);
    let mark = |bot: &Bot, slot: u8| {
        let snap = bot.last.as_ref().expect("a snapshot");
        snap.others.iter().find(|o| o.slot == slot).map(|o| o.pvp)
    };
    assert_eq!(mark(&b, 0), Some(true), "B sees A's mark");
    assert_eq!(mark(&c, 0), Some(true), "and so does C");
    assert_eq!(mark(&a, 1), Some(false), "B is Protected");
    let me = |bot: &Bot| bot.last.as_ref().expect("a snapshot").me.clone();
    assert_eq!(me(&a).pvp, Pvp::Enabled, "A's own HUD knows");

    spread_out(&mut s);
    let mut bots = [&mut a, &mut b, &mut c];
    assert!(shoot(&mut s, &mut bots, 2, 0) <= 0.0, "C can't start on A");
    assert!(shoot(&mut s, &mut bots, 0, 1) > 0.0, "A hurts B");
    assert_eq!(s.world.pilots[0].wanted, 1, "a star for the attack");
    assert!(s.world.pilots[0].combat_t > 0.0, "A is locked in");
    assert!(!me(bots[1]).payback.is_empty(), "B's HUD offers payback");
    assert!(shoot(&mut s, &mut bots, 1, 0) > 0.0, "B fires back");
    assert_eq!(s.world.pilots[1].wanted, 0, "self-defence earns none");
    assert!(
        shoot(&mut s, &mut bots, 2, 1) <= 0.0,
        "C still can't hurt B"
    );

    // The payback window closes 30 s after A's last attack.
    seconds(&mut s, &mut bots, cj_sim::perm::PVP_COMBAT_LOCK);
    assert!(shoot(&mut s, &mut bots, 1, 0) <= 0.0, "B's window closed");
}

/// A PvP player who drops out mid-fight leaves the tug in the field until
/// the combat lock runs out, coming back inside the lock changes nothing,
/// and only after the lock does the tug leave the field.
#[test]
fn a_tug_in_a_combat_lock_stays_in_the_field_when_its_player_leaves() {
    let (mut s, mut a, mut b, mut c) = pvp_trio();
    arm_first(&mut s, &mut a, &mut b, &mut c);
    spread_out(&mut s);
    assert!(shoot(&mut s, &mut [&mut a, &mut b, &mut c], 0, 1) > 0.0);

    s.renet.disconnect_local_client(1, &mut a.client);
    seconds(&mut s, &mut [&mut b, &mut c], 5.0);
    assert!(s.world.pilots[0].ship.alive, "still out there, locked");
    assert!(
        shoot(&mut s, &mut [&mut b, &mut c], 1, 0) > 0.0,
        "B can hit it"
    );

    let mut back = Bot::join(&mut s, 4, 1);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut back, &mut b, &mut c], 3);
    assert_eq!(back.pilot, Some(0));
    assert_eq!(
        s.world.pilots[0].pvp,
        Pvp::Enabled,
        "returning isn't a reset"
    );
    assert!(
        s.world.pilots[0].combat_t > 0.0,
        "nor does it lift the lock"
    );

    s.renet.disconnect_local_client(4, &mut back.client);
    seconds(&mut s, &mut [&mut b, &mut c], cj_sim::perm::PVP_COMBAT_LOCK);
    assert!(!s.world.pilots[0].ship.alive, "gone once the lock ran out");
}
