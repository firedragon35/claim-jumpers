//! The headless dedicated server. It needs no window, graphics or audio
//! device, and builds on Windows and Linux.
//!
//! ```text
//! cj-server [--port N] [--players N] [--seed N] [--mode M] [--careers DIR]
//!           [--record FILE] [--loopback]
//! ```
//!
//! `--mode` is the difficulty of every career on the server (relaxed,
//! standard or hard). Players' careers are kept in `--careers DIR`, by
//! default a `careers` folder in the directory the server runs from.
//!
//! It listens for UDP on every IPv4 interface (`--loopback`: on this machine's
//! loopback interface only, for trying it out without opening it up). Players on the same LAN join
//! with this machine's LAN address; players over the internet need the port
//! forwarded on the router, which some ISPs (CGNAT) don't allow at all. The
//! transport is renet's unsecure netcode: traffic is not encrypted.

use cj_server::{Config, Server};
use std::net::Ipv4Addr;
use std::time::SystemTime;

/// Usage, shown for --help or a bad flag.
const USAGE: &str = "usage: cj-server [--port N] [--players N] [--seed N] [--mode relaxed|standard|hard] [--careers DIR] [--record FILE] [--loopback]";

/// The command line, parsed.
#[derive(Debug)]
struct Args {
    port: u16,
    players: usize,
    seed: u32,
    /// Where to save the shift's recording when it ends.
    record: Option<std::path::PathBuf>,
    /// Listen on the loopback interface only.
    loopback: bool,
    mode: cj_sim::career::Mode,
    careers: std::path::PathBuf,
}

/// Parses the command line; Err holds the message to print.
fn parse(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args {
        port: cj_net::DEFAULT_PORT,
        players: 4,
        seed: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos()),
        record: None,
        loopback: false,
        mode: cj_sim::career::Mode::Standard,
        careers: "careers".into(),
    };
    while let Some(flag) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--port" => {
                a.port = value("--port")?
                    .parse()
                    .map_err(|_| format!("bad --port\n{USAGE}"))?
            }
            "--players" => {
                a.players = value("--players")?
                    .parse()
                    .ok()
                    .filter(|n| (1..=cj_server::MAX_PLAYERS).contains(n))
                    .ok_or_else(|| format!("--players must be 1 to 20\n{USAGE}"))?
            }
            "--seed" => {
                a.seed = value("--seed")?
                    .parse()
                    .map_err(|_| format!("bad --seed\n{USAGE}"))?
            }
            "--record" => a.record = Some(value("--record")?.into()),
            "--mode" => {
                a.mode = cj_sim::career::Mode::from_key(&value("--mode")?).ok_or_else(|| {
                    format!(
                        "--mode must be relaxed, standard or hard
{USAGE}"
                    )
                })?
            }
            "--careers" => a.careers = value("--careers")?.into(),
            "--loopback" => a.loopback = true,
            "--help" | "-h" => return Err(USAGE.to_owned()),
            other => return Err(format!("unknown flag {other}\n{USAGE}")),
        }
    }
    Ok(a)
}

fn main() {
    let args = match parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let ip = if args.loopback {
        Ipv4Addr::LOCALHOST
    } else {
        Ipv4Addr::UNSPECIFIED
    };
    let mut transport = match cj_server::bind_udp(ip, args.port, args.players) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("can't listen on UDP port {}: {e}", args.port);
            std::process::exit(1);
        }
    };
    let mut server = Server::new(Config {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        max_players: args.players,
        seed: args.seed,
        record: args.record.is_some(),
        mode: args.mode,
        careers: Some(args.careers.clone()),
    });
    println!(
        "Claim Jumpers server {} on UDP port {} for up to {} pilots (seed {})",
        env!("CARGO_PKG_VERSION"),
        args.port,
        args.players,
        args.seed
    );

    let ran = cj_server::run(&mut transport, &mut server, |server| {
        for line in server.log.drain(..) {
            println!("{line}");
        }
        let Some(path) = args.record.as_ref().filter(|_| server.world.shift_over) else {
            return true;
        };
        let rec = server.recording().expect("recording is on");
        match std::fs::write(path, cj_net::encode(rec)) {
            Ok(()) => println!(
                "shift over after {} steps; {} entries recorded to {}",
                server.tick(),
                rec.entries.len(),
                path.display()
            ),
            Err(e) => eprintln!("couldn't save the recording to {}: {e}", path.display()),
        }
        false
    });
    for line in server.log.drain(..) {
        println!("{line}");
    }
    if ran.is_err() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse(s.split_whitespace().map(str::to_owned))
    }

    /// Flags parse, and bad ones are refused with the usage line.
    #[test]
    fn flags_parse() {
        let a =
            args("--port 5000 --players 3 --seed 9 --record shift.rec --loopback").expect("valid");
        assert!(a.loopback);
        assert_eq!((a.port, a.players, a.seed), (5000, 3, 9));
        assert_eq!(a.record, Some("shift.rec".into()));
        assert_eq!(args("").expect("defaults").port, cj_net::DEFAULT_PORT);
        assert!(args("--players 0").is_err());
        assert!(args("--players 21").is_err());
        assert_eq!(args("--players 20").expect("valid").players, 20);
        let m = args("--mode hard --careers here").expect("valid");
        assert_eq!(m.mode, cj_sim::career::Mode::Hard);
        assert_eq!(m.careers, std::path::PathBuf::from("here"));
        assert!(args("--mode silly").is_err());
        assert!(args("--port").is_err());
        assert!(args("--bogus").unwrap_err().contains("usage"));
    }
}
