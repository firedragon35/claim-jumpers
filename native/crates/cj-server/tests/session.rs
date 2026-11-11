//! The server end to end over renet's in-memory local clients (the same
//! route the game's host takes): handshakes and refusals, hostile and
//! flooding clients, reconnecting, late joins, two and three pilots in one
//! shift, the per-pilot view, shared sounds, and exact replay from a
//! recording.

use cj_net::{channel, ClientMsg, Reject, ServerMsg, Snapshot};
use cj_server::{replay, Config, Server};
use cj_sim::consts::STEP;
use cj_sim::events::{Emitted, Event, Sfx};
use cj_sim::{Action, Input};
use renet::{ClientId, RenetClient};
use std::time::Duration;

const VERSION: &str = "9.9.9";

fn server(max_players: usize) -> Server {
    Server::new(Config {
        version: VERSION.to_owned(),
        max_players,
        seed: 42,
        record: true,
        mode: cj_sim::career::Mode::Standard,
        careers: None,
    })
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

/// A full shift, a wrong secret and a second connection are each refused.
#[test]
fn full_shifts_wrong_secrets_and_doubles_are_refused() {
    let mut s = server(1);
    let mut a = Bot::join(&mut s, 1, 1);
    a.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a], 3);
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
    assert_eq!(double.rejected, Some(Reject::AlreadyConnected));
    assert_eq!(s.player_count(), 1);
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
    s.renet.disconnect_local_client(2, &mut b.client);
    steps(&mut s, &mut [&mut a], 3);
    let mut back = Bot::join(&mut s, 3, 2);
    back.hello(VERSION, 7);
    steps(&mut s, &mut [&mut a, &mut back], 3);
    assert_eq!(back.pilot, Some(1), "the same pilot as before");
    assert_eq!(s.world.pilots.len(), 2, "no new tug");
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
