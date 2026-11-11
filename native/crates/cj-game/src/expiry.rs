//! The play-test cut-off. Builds handed to play testers stop running on a
//! fixed date, so old builds don't stay in circulation after the test. A
//! later build moves the date by changing `CUT_OFF_UNIX` and
//! `CUT_OFF_TEXT` together.
//!
//! The date is judged by network time from `time.google.com`, so setting
//! the PC's clock back doesn't get past it. When the time server can't be
//! reached (no internet, a LAN-only party, UDP port 123 blocked), the PC's
//! clock is used instead, so offline play keeps working (owner decision
//! 2026-11-11). It is a reminder to fetch a new build, not copy protection.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 1 June 2027 00:00 UTC, in seconds since 1970-01-01 00:00 UTC:
/// 20,970 whole days (57 years with 14 leap days, then January to May).
pub const CUT_OFF_UNIX: u64 = 20_970 * 86_400;

/// The cut-off date as players read it.
pub const CUT_OFF_TEXT: &str = "1 June 2027";

/// The time server the date is checked against (the owner's choice), on
/// the standard NTP port.
const TIME_SERVER: &str = "time.google.com:123";

/// The longest the game waits for network time before it falls back to the
/// PC's clock. Online, an answer takes tens of milliseconds; this only
/// bounds the wait when the network is down or the reply is lost.
const TIME_WAIT: Duration = Duration::from_millis(1500);

/// Seconds from NTP's epoch (1900-01-01) to the Unix epoch (1970-01-01).
const NTP_TO_UNIX: u64 = 2_208_988_800;

/// Whether `now` is at or past the cut-off. A clock set before 1970 has no
/// time since the epoch, and counts as not expired rather than crashing.
pub fn expired(now: SystemTime) -> bool {
    now.duration_since(UNIX_EPOCH)
        .is_ok_and(|since| since.as_secs() >= CUT_OFF_UNIX)
}

/// The time the cut-off is judged by: the time server's when it answers,
/// otherwise the PC's clock. Blocks for at most `TIME_WAIT`.
pub fn now() -> SystemTime {
    network_time().unwrap_or_else(SystemTime::now)
}

/// Asks the time server for the time, giving up after `TIME_WAIT`.
fn network_time() -> Option<SystemTime> {
    let (tx, rx) = mpsc::channel();
    // The name lookup has no timeout of its own, so the query runs on its
    // own thread; if it's slow, the game stops waiting and the thread is
    // left to finish (or fail) on its own.
    std::thread::spawn(move || {
        let _ = tx.send(query(TIME_SERVER));
    });
    rx.recv_timeout(TIME_WAIT).ok().flatten()
}

/// One SNTP exchange (RFC 4330) with `server`: a 48-byte client request,
/// then the reply's transmit time. IPv4 only, like the game's networking.
fn query(server: &str) -> Option<SystemTime> {
    let addr = server.to_socket_addrs().ok()?.find(|a| a.is_ipv4())?;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(TIME_WAIT)).ok()?;
    // A connected socket only accepts datagrams from the server's address.
    socket.connect(addr).ok()?;
    // A random transmit stamp, which the server copies into its reply's
    // origin stamp: a stray or forged reply without it is refused.
    let nonce = RandomState::new().build_hasher().finish();
    let mut request = [0u8; 48];
    request[0] = 0x23; // leap indicator 0, version 4, mode 3 (client)
    request[40..48].copy_from_slice(&nonce.to_be_bytes());
    socket.send(&request).ok()?;
    // Room for the optional key and digest fields some servers append.
    let mut reply = [0u8; 68];
    let n = socket.recv(&mut reply).ok()?;
    parse_reply(&reply[..n], nonce)
}

/// The server's transmit time from an SNTP reply, or `None` when the reply
/// isn't a usable answer to the request that carried `nonce`.
fn parse_reply(reply: &[u8], nonce: u64) -> Option<SystemTime> {
    if reply.len() < 48 {
        return None;
    }
    let leap = reply[0] >> 6;
    let mode = reply[0] & 0x07;
    let stratum = reply[1];
    // Leap indicator 3 means the server's own clock isn't synchronized;
    // stratum 0 is a refusal ("kiss-o'-death"), and above 15 is invalid.
    if leap == 3 || mode != 4 || stratum == 0 || stratum > 15 {
        return None;
    }
    if reply[24..32] != nonce.to_be_bytes() {
        return None;
    }
    let seconds = u64::from(u32::from_be_bytes(reply[40..44].try_into().ok()?));
    if seconds == 0 {
        return None;
    }
    // NTP seconds wrap every 2^32 s (next on 7 February 2036). A value that
    // would fall before 1970 belongs to the next era.
    let seconds = if seconds < NTP_TO_UNIX {
        seconds + (1 << 32)
    } else {
        seconds
    };
    Some(UNIX_EPOCH + Duration::from_secs(seconds - NTP_TO_UNIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cut_off_is_1_june_2027_utc() {
        // The same instant as Python's
        // datetime(2027, 6, 1, tzinfo=timezone.utc).timestamp().
        assert_eq!(CUT_OFF_UNIX, 1_811_808_000);
    }

    #[test]
    fn runs_until_the_last_second_before_the_cut_off() {
        let at = UNIX_EPOCH + Duration::from_secs(CUT_OFF_UNIX);
        assert!(!expired(at - Duration::from_secs(1)));
        assert!(expired(at));
        assert!(expired(at + Duration::from_secs(86_400 * 365)));
    }

    #[test]
    fn a_clock_before_1970_does_not_expire() {
        assert!(!expired(UNIX_EPOCH - Duration::from_secs(1)));
        assert!(!expired(UNIX_EPOCH));
    }

    /// A server reply (leap 0, version 4, mode 4, stratum 1) answering
    /// `nonce`, with `ntp_seconds` as its transmit time.
    fn reply(nonce: u64, ntp_seconds: u32) -> [u8; 48] {
        let mut r = [0u8; 48];
        r[0] = 0x24;
        r[1] = 1;
        r[24..32].copy_from_slice(&nonce.to_be_bytes());
        r[40..44].copy_from_slice(&ntp_seconds.to_be_bytes());
        r
    }

    #[test]
    fn a_good_reply_gives_the_servers_time() {
        // 1 June 2027 00:00 UTC in NTP seconds.
        let ntp = (CUT_OFF_UNIX + NTP_TO_UNIX) as u32;
        let t = parse_reply(&reply(7, ntp), 7).expect("a good reply");
        assert_eq!(t, UNIX_EPOCH + Duration::from_secs(CUT_OFF_UNIX));
        assert!(expired(t));
    }

    #[test]
    fn times_after_the_2036_wrap_count_from_the_next_era() {
        // NTP second 0 of era 1 is 7 February 2036 06:28:16 UTC.
        let t = parse_reply(&reply(7, 1), 7).expect("a good reply");
        let unix = t.duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert_eq!(unix, (1 << 32) + 1 - NTP_TO_UNIX);
    }

    #[test]
    fn unusable_replies_are_refused() {
        let good = reply(7, 3_900_000_000);
        assert!(parse_reply(&good, 7).is_some());
        // Not an answer to this request.
        assert!(parse_reply(&good, 8).is_none());
        // Too short to hold a transmit time.
        assert!(parse_reply(&good[..47], 7).is_none());
        // A client packet echoed back, not a server reply.
        let mut r = good;
        r[0] = 0x23;
        assert!(parse_reply(&r, 7).is_none());
        // The server's own clock is unsynchronized.
        let mut r = good;
        r[0] |= 0xC0;
        assert!(parse_reply(&r, 7).is_none());
        // A kiss-o'-death refusal.
        let mut r = good;
        r[1] = 0;
        assert!(parse_reply(&r, 7).is_none());
        // No transmit time.
        assert!(parse_reply(&reply(7, 0), 7).is_none());
    }

    /// Asks the real time server; run by hand with
    /// `cargo test -p cj-game -- --ignored network`, as CI may have no
    /// internet. A day's leeway: a PC clock a few minutes out is common,
    /// while a parsing mistake would be off by years.
    #[test]
    #[ignore]
    fn network_time_is_close_to_the_pcs_clock() {
        let net = network_time().expect("time.google.com answered");
        let pc = SystemTime::now();
        let skew = net
            .duration_since(pc)
            .or_else(|_| pc.duration_since(net))
            .unwrap();
        assert!(skew < Duration::from_secs(86_400), "skew {skew:?}");
    }
}
