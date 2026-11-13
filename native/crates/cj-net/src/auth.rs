//! Logging in without sending the secret (IMP-023).
//!
//! renet's unsecure transport doesn't encrypt anything, so a Hello that
//! carried the player's secret could be copied by anyone on the path (the
//! same Wi-Fi, say) and replayed to take over that player's career. Instead:
//!
//! 1. Each install keeps a 16-byte secret; its SHA-256 is its *print*. A
//!    server keeps only prints (`<player>.key`), never secrets.
//! 2. On connecting, the server sends a fresh random nonce
//!    ([`crate::ServerMsg::Challenge`]).
//! 3. The Hello carries a [`proof`]: a hash of that nonce, the player id and
//!    the print. A copied proof is useless on the next connection, whose
//!    nonce is different.
//! 4. A server that has never seen the player asks for the print once
//!    ([`crate::ServerMsg::Introduce`]) and binds it to the career.
//!
//! Limits: the print crosses the network on that first join to each server,
//! so someone watching that one join could later claim the career; and none
//! of this encrypts the traffic or stops someone who can alter packets in
//! flight. It stops the easy attack: replaying a captured login.

use crate::PlayerId;
use sha2::{Digest, Sha256};
use std::hash::{BuildHasher, Hasher};
use std::time::SystemTime;

/// A secret's SHA-256: what a server stores and the proof is keyed with.
pub type Print = [u8; 32];

/// Separates these hashes from any other SHA-256 use of the same bytes.
const DOMAIN: &[u8] = b"claim-jumpers login v1";

/// The print of `secret`. The secret is 16 random bytes, not something a
/// person chose, so a plain hash can't be guessed backwards and a slow
/// password hash would add nothing.
pub fn fingerprint(secret: &[u8; 16]) -> Print {
    Sha256::digest(secret).into()
}

/// The answer to a server's `nonce`: shows the client holds `print` for
/// `player` without sending the print.
pub fn proof(nonce: &[u8; 16], player: &PlayerId, print: &Print) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(nonce);
    h.update(player);
    h.update(print);
    h.finalize().into()
}

/// Compares two proofs in time that doesn't depend on where they differ,
/// so a server's reply times can't be used to guess a proof byte by byte.
pub fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 16 bytes that are hard to guess: std's per-process hash keys come from
/// the operating system's random source, mixed with the clock and a
/// per-call counter, then hashed. Not a cryptographic generator, but ids,
/// secrets and nonces only need to be unlikely to collide or be predicted;
/// in particular a server's nonces don't repeat across restarts, which a
/// seeded game generator's would.
pub fn random_bytes() -> [u8; 16] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let mut h = Sha256::new();
    for i in 0..2u64 {
        let mut k = std::hash::RandomState::new().build_hasher();
        k.write_u64(i);
        k.write_u64(CALLS.fetch_add(1, Ordering::Relaxed));
        k.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.update(k.finish().to_le_bytes());
    }
    let full: [u8; 32] = h.finalize().into();
    full[..16].try_into().expect("16 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A proof depends on the nonce, the player and the print, so one copied
    /// from an earlier connection (another nonce) doesn't log in.
    #[test]
    fn a_proof_is_bound_to_its_nonce_player_and_print() {
        let print = fingerprint(&[9; 16]);
        let p = proof(&[1; 16], &[7; 16], &print);
        assert!(same(&p, &proof(&[1; 16], &[7; 16], &print)));
        assert!(
            !same(&p, &proof(&[2; 16], &[7; 16], &print)),
            "another nonce"
        );
        assert!(
            !same(&p, &proof(&[1; 16], &[8; 16], &print)),
            "another player"
        );
        let other = fingerprint(&[10; 16]);
        assert!(
            !same(&p, &proof(&[1; 16], &[7; 16], &other)),
            "another secret"
        );
        assert_ne!(print[..16], [9; 16], "the print isn't the secret");
    }

    #[test]
    fn random_bytes_differ_call_to_call() {
        let a = random_bytes();
        assert_ne!(a, random_bytes());
        assert_ne!(a, [0; 16]);
    }
}
