//! The Claim Jumpers network protocol (CJN-15): the messages a client and a
//! server exchange, their limits and validation, and snapshots of the world
//! as one pilot may see it.
//!
//! It is transport-agnostic: renet carries the bytes (cj-server and the
//! game), and this crate only shapes them. Everything a client sends is
//! untrusted, so the server decodes it with [`decode`] (which bounds its
//! size) and passes held controls through [`clean_input`].

pub mod client;
pub mod snapshot;

pub use snapshot::{Blip, BlipKind, Known, Look, Snapshot};

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

/// A player's public id on a server. Paired with a secret (see
/// [`ClientMsg::Hello`]) it lets the same install reclaim its pilot. It
/// proves nothing about who the player is, and with renet's unsecure
/// transport the secret crosses the network unencrypted.
pub type PlayerId = [u8; 16];

/// What a client sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// The first message after connecting.
    Hello {
        /// The game's version; the server requires an exact match.
        game_version: String,
        player: PlayerId,
        secret: [u8; 16],
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
    /// This player is already connected from somewhere else.
    AlreadyConnected,
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
            Reject::AlreadyConnected => "Your pilot is already flying on this server.".to_owned(),
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
pub fn clean_name(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | '\''))
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
            secret: [9; 16],
            pilot: "Ace".to_owned(),
            view: (1280, 720),
        };
        assert_eq!(decode::<ClientMsg>(&encode(&hello)), Ok(hello));
        let reject = Reject::VersionMismatch {
            server: "1.3.0".to_owned(),
        };
        let back = decode::<ServerMsg>(&encode(&ServerMsg::Reject(reject.clone())));
        assert!(matches!(back, Ok(ServerMsg::Reject(r)) if r == reject));
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
    }
}
