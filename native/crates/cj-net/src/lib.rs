//! The Claim Jumpers network protocol: the messages a client and a
//! server exchange, their limits and validation, and snapshots of the world
//! as one pilot may see it.
//!
//! It is transport-agnostic: renet carries the bytes (cj-server and the
//! game), and this crate only shapes them. Everything a client sends is
//! untrusted, so the server decodes it with [`decode`] (which bounds its
//! size) and passes held controls through [`clean_input`].

pub mod auth;
pub mod client;
pub mod snapshot;

pub use snapshot::{Blip, BlipKind, Known, Look, Snapshot};

use cj_sim::career::{Career, Outcome};
use cj_sim::events::Emitted;
use cj_sim::{Action, Input};
use serde::{Deserialize, Serialize};

/// The netcode protocol id. It is fixed for the game, so every client's
/// handshake reaches the server and a version mismatch is answered with a
/// reason; renet silently drops a connection whose protocol id differs.
pub const PROTOCOL_ID: u64 = 0x434c_4149_4d4a_4d50; // "CLAIMJMP"

/// The default UDP port a server listens on.
pub const DEFAULT_PORT: u16 = 47820;

/// How far away another player's sounds carry, in world units: the server
/// sends them to clients within this distance, and the game plays them
/// quieter the further away they are.
pub const HEARING: f64 = 1600.0;

/// The most steps of held controls one [`ClientMsg::Input`] carries.
pub const INPUTS_PER_MESSAGE: usize = 4;

/// The largest message either side accepts, in bytes. Bigger ones are
/// dropped before any decoding work.
pub const MAX_MESSAGE: usize = 64 * 1024;

/// The renet channels each message kind travels on (renet's default
/// channel numbering).
pub mod channel {
    /// Held controls and snapshots: lost or late ones are simply replaced.
    pub const UNRELIABLE: u8 = 0;
    /// The handshake, cable commands and events: delivered, in order.
    pub const RELIABLE: u8 = 2;
}

/// A player's public id on a server. Paired with a secret, proved without
/// sending it (see [`auth`]), it lets the same install reclaim its pilot.
/// It proves nothing about who the player is.
pub type PlayerId = [u8; 16];

/// What a client sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// The first message after the server's [`ServerMsg::Challenge`].
    Hello {
        /// The game's version; the server requires an exact match.
        game_version: String,
        player: PlayerId,
        /// [`auth::proof`] of the challenge's nonce: the secret itself
        /// never crosses the network.
        proof: [u8; 32],
        /// The pilot name to show; the server cleans it ([`clean_name`]).
        pilot: String,
        /// The client's window size in logical pixels, which sets how much
        /// of the field it may see ([`clean_view`]).
        view: (u16, u16),
    },
    /// Held controls, one per step: `inputs` are steps `seq - n + 1` to
    /// `seq`, oldest first, so a lost packet is covered by the next (at most
    /// [`INPUTS_PER_MESSAGE`]). The server applies each step's exactly once.
    Input { seq: u32, inputs: Vec<Input> },
    /// A one-shot cable command, numbered so duplicates can be dropped.
    Action { seq: u32, action: Action },
    /// Before Hello: asks how full the server is ([`ServerMsg::Info`]).
    /// Asking again keeps the connection open while the player decides.
    Query,
    /// During the break between shifts: this player is ready for the next.
    Ready,
    /// A line for the public chat; the server cleans it ([`clean_chat`]).
    Chat { text: String },
    /// In the break: a shop request or a belt vote ([`ShopReq`]).
    Shop(ShopReq),
    /// The answer to [`ServerMsg::Introduce`]: this player's
    /// [`auth::Print`], sent once to a server that has never seen them.
    Introduce { print: auth::Print },
}

/// What a player asks the shop in the break. The server checks each one
/// against the player's career with the same rules single player uses, and
/// answers with [`ServerMsg::Shop`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShopReq {
    /// Pay off 1000 of the loan, or `all` of it.
    Pay {
        all: bool,
    },
    Buy(cj_sim::parts::Part),
    Mount(cj_sim::parts::Part),
    Unmount(cj_sim::parts::Part),
    Sentry,
    Level(cj_sim::parts::System),
    /// Take or drop contract offer number `n`.
    Offer(u8),
    Crew(cj_sim::field::Crew),
    Truce,
    SellCore,
    ReprogramCore,
    /// This player's vote for the next shift's belt.
    Vote(cj_sim::field::Belt),
    /// Insure the next shift's cargo.
    Insure,
}

impl ShopReq {
    /// Applies the request to `c` with the shop's rules, the one place they
    /// are dispatched: the server runs it for a player's request, and single
    /// player runs it for a shop button. `Ok` holds the shop's sentence and,
    /// for a payment that cleared the loan, the outcome (a trade-up, already
    /// applied, or a paid-off fleet, which the caller ends the career for).
    /// `Err` holds the shop's refusal. A belt vote only picks the belt here;
    /// counting votes is the server's job.
    pub fn apply(self, c: &mut Career) -> Result<(String, Option<Outcome>), String> {
        // Everything but a payment is a plain sentence with no outcome.
        let plain = |r: cj_sim::career::ShopResult| r.map(|msg| (msg, None));
        match self {
            ShopReq::Pay { all } => {
                let limit = if all { f64::INFINITY } else { 1000.0 };
                match c.pay_extra(limit)? {
                    // pay_extra leaves the sentence empty when the loan is
                    // cleared, so a trade-up is named here (online it is the
                    // only word the player gets; single player shows a report).
                    (_, Some(o @ Outcome::TradedUp { .. })) => {
                        let next = c.tug();
                        Ok((
                            format!(
                                "Loan paid off. Traded up: {} \"{}\". New loan: {} cr.",
                                next.mk,
                                next.name,
                                cj_sim::world::fmt_credits(c.loan)
                            ),
                            Some(o),
                        ))
                    }
                    paid => Ok(paid),
                }
            }
            ShopReq::Buy(p) => plain(c.buy_part(p)),
            ShopReq::Mount(p) => plain(c.mount(p)),
            ShopReq::Unmount(p) => plain(c.unmount(p)),
            ShopReq::Sentry => plain(c.buy_sentry()),
            ShopReq::Level(s) => plain(c.buy_level(s)),
            // The offer number comes from the network: check it before
            // toggle_offer indexes the offers with it.
            ShopReq::Offer(n) if usize::from(n) < c.offers.len() => {
                Ok((c.toggle_offer(usize::from(n)), None))
            }
            ShopReq::Offer(_) => Err("There's no such contract.".to_owned()),
            ShopReq::Crew(crew) => Ok((c.toggle_crew(crew), None)),
            ShopReq::Truce => plain(c.buy_truce()),
            ShopReq::SellCore => plain(c.sell_core()),
            ShopReq::ReprogramCore => plain(c.reprogram_core()),
            ShopReq::Vote(b) => plain(c.pick_belt(b)),
            ShopReq::Insure => plain(c.buy_insurance()),
        }
    }
}

/// What the server sends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    /// The handshake is accepted, or a new shift has started: this client
    /// flies `pilot`, and the shift's snapshots start at server step
    /// `from_tick` (older ones in flight belong to the last shift).
    Welcome { pilot: u8, from_tick: u32 },
    /// The handshake is refused; the server disconnects next.
    Reject(Reject),
    /// The world as this client's pilot may see it.
    Snapshot(Box<Snapshot>),
    /// Events since the last batch that this client may hear.
    Events(Vec<Emitted>),
    /// Fixed outlines of rocks and items this client is seeing for the
    /// first time (reliable, so each is sent only once).
    Looks(Vec<Look>),
    /// The answer to [`ClientMsg::Query`].
    Info(Info),
    /// The handshake is accepted, but the shift is nearly over: this client
    /// waits in the lobby and is welcomed to the next shift.
    Waiting,
    /// Who is on the server and how the shift or break stands, sent every
    /// second (and more often in the break).
    Lobby(Lobby),
    /// A public chat line, sent to everyone connected.
    Chat(ChatLine),
    /// The answer to a [`ClientMsg::Shop`]: whether it went through, the
    /// shop's sentence about it, and the career as it now stands (save
    /// format). Also sent, with an empty sentence, to give a player their
    /// career for the break when there is no report for them.
    Shop(ShopReply),
    /// The shift this player flew is settled: what it paid and what it
    /// cost, and their career afterwards (in the save format; a career that
    /// ended in repossession or a paid-off fleet is already the fresh one).
    Settled(Box<Settled>),
    /// Sent as a connection opens: the nonce its Hello must prove (see
    /// [`auth`]). Each connection gets a new one.
    Challenge { nonce: [u8; 16] },
    /// The Hello's player is new here: send [`ClientMsg::Introduce`].
    Introduce,
}

/// See [`ServerMsg::Shop`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShopReply {
    pub ok: bool,
    pub msg: String,
    pub career: String,
}

/// One player's end of a shift (see [`ServerMsg::Settled`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settled {
    pub settlement: cj_sim::career::Settlement,
    /// `Career::to_json`, the single-player save format.
    pub career: String,
}

/// One line of the public chat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatLine {
    /// The sender's name as the server knows it.
    pub from: String,
    /// The sender's pilot slot (for its colour); `None` while they wait
    /// in the lobby.
    pub slot: Option<u8>,
    pub text: String,
}

/// How full a server is, for the Join screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    /// The game version it runs.
    pub version: String,
    /// Players connected now (flying or waiting in the lobby).
    pub connected: u8,
    /// The player limit.
    pub max: u8,
    /// Every slot is taken, so a new player would be refused.
    pub full: bool,
    /// The host allows PvP on it.
    pub pvp: bool,
}

/// Where a server is between shifts and shifts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// A shift is on, with `time_left` seconds to go.
    Shift { time_left: f32 },
    /// The break before the next shift, which starts when everyone is
    /// ready or after `seconds_left`.
    Break { seconds_left: f32 },
}

/// One player in the lobby list (and the Tab list in a shift).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LobbyPlayer {
    pub name: String,
    /// Their pilot in this shift; `None` while waiting for the next one.
    pub slot: Option<u8>,
    /// Credits banked this shift.
    pub banked: i64,
    /// Rivals scrapped and other players' tugs wrecked (PvP) this shift.
    pub kills: u32,
    pub deaths: u32,
    /// Disconnected, with their tug out of the field until they return.
    pub away: bool,
    /// Ready for the next shift (in the break).
    pub ready: bool,
}

/// The server's lobby list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lobby {
    /// The server step it was sent at (lists travel unreliably, so an old
    /// one can arrive after a newer one, or after the next shift's Welcome).
    pub tick: u32,
    /// Shifts started on this server, counting the current one.
    pub shift_no: u32,
    pub phase: Phase,
    pub players: Vec<LobbyPlayer>,
    pub max: u8,
    /// The belt votes so far in the break, by belt, most first.
    pub votes: Vec<(cj_sim::field::Belt, u8)>,
}

/// Why a server refused a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reject {
    /// The client runs a different game version from the server's.
    VersionMismatch { server: String },
    /// Every pilot slot is taken.
    Full,
    /// The first message was not a valid `Hello`.
    BadHello,
    /// The player id is known here, but the secret doesn't match it.
    WrongSecret,
    /// This player has joined again on another connection, which now has
    /// their pilot. Sent to the connection it replaced.
    AlreadyConnected,
    /// The server couldn't check or save what proves the career is this
    /// player's (a disk fault on the host), so it lets nobody in on it.
    ServerError,
    /// The server has taken as many players new to it as it takes in a
    /// minute; players it already knows still get in. Kept last: postcard
    /// numbers variants in order.
    Busy,
}

impl Reject {
    /// A sentence to show a player whose game is `mine`.
    pub fn reason(&self, mine: &str) -> String {
        match self {
            Reject::VersionMismatch { server } => format!(
                "The server runs Claim Jumpers {server} and you have {mine}. Both must be the same version."
            ),
            Reject::Full => "The server is full.".to_owned(),
            Reject::BadHello => "The server didn't understand this client.".to_owned(),
            Reject::WrongSecret => {
                "This server knows your pilot from another install, so it won't let this one in."
                    .to_owned()
            }
            Reject::AlreadyConnected => {
                "Your pilot joined this server again from another game, so this one was disconnected."
                    .to_owned()
            }
            Reject::ServerError => {
                "The server couldn't save your pilot, so it can't let you in right now. Ask the host to check its disk."
                    .to_owned()
            }
            Reject::Busy => {
                "The server is taking a lot of new pilots right now. Try again in a minute."
                    .to_owned()
            }
        }
    }
}

/// Why a received message was dropped.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Bigger than [`MAX_MESSAGE`]; holds the size in bytes.
    TooLarge(usize),
    /// Not a valid message.
    Malformed,
}

/// Encodes a protocol message.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    // Protocol types are plain data with no failing serializers.
    postcard::to_stdvec(msg).expect("protocol messages always encode")
}

/// Decodes a received message, refusing oversized ones before any work.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, DecodeError> {
    if bytes.len() > MAX_MESSAGE {
        return Err(DecodeError::TooLarge(bytes.len()));
    }
    postcard::from_bytes(bytes).map_err(|_| DecodeError::Malformed)
}

/// Held controls from the network, clamped to their ranges, or None when a
/// value is not a finite number (a broken or hostile client).
pub fn clean_input(i: Input) -> Option<Input> {
    if [i.thrust, i.reverse, i.turn, i.strafe]
        .iter()
        .any(|v| !v.is_finite())
    {
        return None;
    }
    Some(Input {
        thrust: i.thrust.clamp(0.0, 1.0),
        reverse: i.reverse.clamp(0.0, 1.0),
        turn: i.turn.clamp(-1.0, 1.0),
        strafe: i.strafe.clamp(-1.0, 1.0),
        fire: i.fire,
        burn: i.burn,
    })
}

/// A client's window size, clamped to what a real screen could be (so a
/// claimed giant window can't reveal the whole field).
pub fn clean_view(v: (u16, u16)) -> (f64, f64) {
    (
        f64::from(v.0.clamp(640, 3840)),
        f64::from(v.1.clamp(360, 2160)),
    )
}

/// Characters a pilot name may hold, after cleaning.
pub const NAME_MAX: usize = 16;

/// The longest chat line, in characters.
pub const CHAT_MAX: usize = 120;

/// A chat line as the server passes it on: printable characters the game's
/// fonts can draw (ASCII and Latin letters), spaces collapsed, at most
/// [`CHAT_MAX`] characters. Empty means there's nothing to send.
pub fn clean_chat(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|&c| {
            c == ' ' || c.is_ascii_graphic() || (c.is_alphabetic() && u32::from(c) < 0x250)
        })
        .collect();
    let collapsed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .chars()
        .take(CHAT_MAX)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// cleanName: letters, numbers, space, _ - . and '; spaces collapsed; at
/// most [`NAME_MAX`] characters. The game's name entry and the server use
/// the same rule.
///
/// Letters and numbers stop where chat lines do (ASCII and Latin, below
/// U+0250): the game's fonts draw nothing else, so other scripts would show
/// as blanks on everyone's screen, and Cyrillic or Greek lookalikes would
/// let one player pass as another ("Аce" for "Ace").
pub fn clean_name(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .filter(|&c| {
            (c.is_alphanumeric() && u32::from(c) < 0x250)
                || matches!(c, ' ' | '_' | '-' | '.' | '\'')
        })
        .collect();
    let collapsed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .chars()
        .take(NAME_MAX)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hostile bytes at the trust boundary: random strings, and valid
    /// client messages with bytes flipped, cut short or extended, must
    /// decode to a message or an error, never a panic. Anything that does
    /// decode still passes through the cleaners the server applies.
    #[test]
    fn random_and_mutated_bytes_never_panic_the_decoder() {
        let mut rng = cj_sim::rng::Rng::seeded(84);
        let valid: Vec<Vec<u8>> = vec![
            encode(&ClientMsg::Hello {
                game_version: "1.15.0".to_owned(),
                player: [7; 16],
                proof: [9; 32],
                pilot: "Tugger-1234".to_owned(),
                view: (1280, 720),
            }),
            encode(&ClientMsg::Input {
                seq: 42,
                inputs: vec![Input::default(); INPUTS_PER_MESSAGE],
            }),
            encode(&ClientMsg::Chat {
                text: "hello field".to_owned(),
            }),
            encode(&ClientMsg::Query),
            encode(&ClientMsg::Introduce { print: [3; 32] }),
        ];
        let byte = |rng: &mut cj_sim::rng::Rng| rng.int(0, 255) as u8;
        for round in 0..100_000 {
            let bytes: Vec<u8> = if round % 2 == 0 {
                let n = rng.int(0, 96) as usize;
                (0..n).map(|_| byte(&mut rng)).collect()
            } else {
                let mut b = valid[round % valid.len()].clone();
                match rng.int(0, 2) {
                    0 if !b.is_empty() => {
                        let i = rng.int(0, b.len() as i32 - 1) as usize;
                        b[i] = byte(&mut rng);
                    }
                    1 => b.truncate(rng.int(0, b.len() as i32) as usize),
                    _ => b.extend((0..rng.int(1, 8)).map(|_| byte(&mut rng))),
                }
                b
            };
            if let Ok(msg) = decode::<ClientMsg>(&bytes) {
                match msg {
                    ClientMsg::Input { inputs, .. } => {
                        inputs.into_iter().for_each(|i| {
                            let _ = clean_input(i);
                        });
                    }
                    ClientMsg::Chat { text } => {
                        let _ = clean_chat(&text);
                    }
                    ClientMsg::Hello { pilot, view, .. } => {
                        let _ = (clean_name(&pilot), clean_view(view));
                    }
                    _ => {}
                }
            }
        }
    }

    /// Chat lines lose control characters and symbols the fonts can't
    /// draw, collapse their spaces and are cut to length.
    #[test]
    fn chat_lines_are_cleaned() {
        assert_eq!(clean_chat("  hi\tthere\n\u{7}  all "), "hi there all");
        assert_eq!(clean_chat("caf\u{e9} \u{1f600}!"), "caf\u{e9} !");
        assert_eq!(clean_chat(&"x".repeat(500)).len(), CHAT_MAX);
        assert_eq!(clean_chat(" \n "), "");
    }

    /// Both directions' messages survive encoding.
    #[test]
    fn messages_round_trip() {
        let hello = ClientMsg::Hello {
            game_version: "1.3.0".to_owned(),
            player: [7; 16],
            proof: [9; 32],
            pilot: "Ace".to_owned(),
            view: (1280, 720),
        };
        assert_eq!(decode::<ClientMsg>(&encode(&hello)), Ok(hello));
        let reject = Reject::VersionMismatch {
            server: "1.3.0".to_owned(),
        };
        let back = decode::<ServerMsg>(&encode(&ServerMsg::Reject(reject.clone())));
        assert!(matches!(back, Ok(ServerMsg::Reject(r)) if r == reject));
        let back = decode::<ServerMsg>(&encode(&ServerMsg::Reject(Reject::ServerError)));
        assert!(matches!(back, Ok(ServerMsg::Reject(Reject::ServerError))));
        let back = decode::<ServerMsg>(&encode(&ServerMsg::Reject(Reject::Busy)));
        assert!(matches!(back, Ok(ServerMsg::Reject(Reject::Busy))));
    }

    /// Oversized and garbage messages are refused.
    #[test]
    fn bad_bytes_are_refused() {
        let huge = vec![0u8; MAX_MESSAGE + 1];
        assert_eq!(
            decode::<ClientMsg>(&huge),
            Err(DecodeError::TooLarge(MAX_MESSAGE + 1))
        );
        assert_eq!(
            decode::<ClientMsg>(&[0xff, 0xff, 0xff]),
            Err(DecodeError::Malformed)
        );
        assert_eq!(decode::<ClientMsg>(&[]), Err(DecodeError::Malformed));
    }

    /// Controls are clamped to their ranges; non-finite ones are refused.
    #[test]
    fn inputs_are_clamped_and_nan_refused() {
        let wild = Input {
            thrust: 5.0,
            reverse: -2.0,
            turn: -9.0,
            strafe: 3.0,
            ..Input::default()
        };
        let clean = clean_input(wild).expect("finite");
        assert_eq!(
            (clean.thrust, clean.reverse, clean.turn, clean.strafe),
            (1.0, 0.0, -1.0, 1.0)
        );
        let nan = Input {
            turn: f64::NAN,
            ..Input::default()
        };
        assert_eq!(clean_input(nan), None);
        let inf = Input {
            thrust: f64::INFINITY,
            ..Input::default()
        };
        assert_eq!(clean_input(inf), None);
    }

    /// Paying off the whole loan trades up and says so: the sentence is all
    /// an online player hears about it.
    #[test]
    fn paying_off_the_loan_names_the_trade_up() {
        let mut c = Career::new(cj_sim::career::Mode::Standard);
        c.wallet = c.loan + 500.0;
        let (msg, outcome) = ShopReq::Pay { all: true }.apply(&mut c).expect("paid");
        assert!(matches!(outcome, Some(Outcome::TradedUp { from: 0, .. })));
        assert!(msg.contains("Traded up: Mk II"), "{msg}");
        assert_eq!(c.tug_class, 1);
        assert_eq!(c.wallet, 500.0, "only the loan was paid");
    }

    /// An offer number past the offers is refused, not used as an index.
    #[test]
    fn a_missing_offer_is_refused() {
        let mut c = Career::new(cj_sim::career::Mode::Standard);
        assert!(ShopReq::Offer(200).apply(&mut c).is_err());
    }

    /// A claimed window is clamped to a real screen's size.
    #[test]
    fn views_are_clamped() {
        assert_eq!(clean_view((65535, 1)), (3840.0, 360.0));
        assert_eq!(clean_view((1280, 720)), (1280.0, 720.0));
    }

    /// Names keep letters, numbers and a few marks, collapse spaces, and fit.
    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Ace   <b>Pilot</b>!!  "), "Ace bPilotb");
        assert_eq!(clean_name("abcdefghijklmnopqrstuvwxyz"), "abcdefghijklmnop");
        assert_eq!(clean_name("O'Neil-2.0_x"), "O'Neil-2.0_x");
        // Latin accents stay; scripts the fonts can't draw go.
        assert_eq!(clean_name("Zoë Ångström"), "Zoë Ångström");
        assert_eq!(clean_name("\u{410}ce"), "ce", "a Cyrillic А");
        assert_eq!(clean_name("Пётр"), "");
        assert_eq!(clean_name("鷹 Hawk"), "Hawk");
    }
}
