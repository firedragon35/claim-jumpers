//! Bundled dialogue: exact lines, reusable dynamic phrases, and number words.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Asset {
    pub system: bool,
    pub text: &'static str,
    pub wav: &'static [u8],
}

include!("recording_index.rs");

/// A single cancellable utterance on the existing audio stream.
pub struct Playback {
    samples: Vec<f32>,
    position: f64,
    generation: Arc<AtomicU64>,
    tag: u64,
}

impl Playback {
    pub fn new(samples: Vec<f32>, generation: Arc<AtomicU64>, tag: u64) -> Self {
        Self {
            samples,
            position: 0.0,
            generation,
            tag,
        }
    }

    pub fn sample(&mut self, output_rate: f32) -> Option<f32> {
        if self.generation.load(Ordering::Relaxed) != self.tag
            || self.position >= self.samples.len() as f64
        {
            return None;
        }
        let i = self.position as usize;
        let fraction = (self.position - i as f64) as f32;
        let a = self.samples[i];
        let b = self.samples.get(i + 1).copied().unwrap_or(0.0);
        self.position += 24_000.0 / f64::from(output_rate);
        Some(a + (b - a) * fraction)
    }
}

/// Decode only the PCM16 mono 24 kHz format shipped by this asset pack.
pub fn decode(wav: &[u8]) -> Option<Vec<f32>> {
    if wav.get(..4)? != b"RIFF" || wav.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut format_ok = false;
    let mut data = None;
    while offset.checked_add(8)? <= wav.len() {
        let tag = wav.get(offset..offset + 4)?;
        let size = u32::from_le_bytes(wav.get(offset + 4..offset + 8)?.try_into().ok()?) as usize;
        offset += 8;
        let chunk = wav.get(offset..offset.checked_add(size)?)?;
        if tag == b"fmt " {
            format_ok = chunk.len() >= 16
                && chunk[0..4] == [1, 0, 1, 0]
                && chunk[4..8] == 24_000u32.to_le_bytes()
                && chunk[14..16] == [16, 0];
        } else if tag == b"data" {
            data = Some(chunk);
        }
        offset = offset.checked_add(size)?.checked_add(size % 2)?;
    }
    let data = data?;
    if !format_ok || data.is_empty() || !data.len().is_multiple_of(2) {
        return None;
    }
    Some(
        data.as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect(),
    )
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '"' | '\u{201c}' | '\u{201d}'))
        .collect()
}

fn number_words(n: u64, words: &mut Vec<&'static str>) {
    const SMALL: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    for (scale, word) in [
        (1_000_000_000_000_000_000, "quintillion"),
        (1_000_000_000_000_000, "quadrillion"),
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1_000, "thousand"),
        (100, "hundred"),
    ] {
        if n >= scale {
            number_words(n / scale, words);
            words.push(word);
            if !n.is_multiple_of(scale) {
                number_words(n % scale, words);
            }
            return;
        }
    }
    if n < 20 {
        words.push(SMALL[n as usize]);
    } else {
        words.push(
            [
                "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
            ][(n / 10 - 2) as usize],
        );
        if !n.is_multiple_of(10) {
            words.push(SMALL[(n % 10) as usize]);
        }
    }
}

/// Resolve a complete line first; changing amounts use the same recorded voice.
pub fn plan(text: &str, system: bool) -> Option<Vec<usize>> {
    if let Some(i) = ASSETS
        .iter()
        .position(|a| a.system == system && a.text == text)
    {
        return Some(vec![i]);
    }
    if system {
        return None;
    }
    let text = normalized(text);
    let mut rest = text.as_str();
    let mut out = Vec::new();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let matched = ASSETS
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.system)
            .filter_map(|(i, a)| {
                let t = normalized(a.text);
                let t = t.trim();
                (!t.is_empty() && rest.starts_with(t)).then_some((i, t.len()))
            })
            .max_by_key(|(_, len)| *len);
        if let Some((i, len)) = matched {
            out.push(i);
            rest = &rest[len..];
        } else if rest.starts_with(|c: char| c.is_ascii_digit())
            || rest.starts_with('-') && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
        {
            let sign = usize::from(rest.starts_with('-'));
            let len = sign
                + rest[sign..]
                    .bytes()
                    .take_while(|b| b.is_ascii_digit() || *b == b',')
                    .count();
            let number: i64 = rest[..len].replace(',', "").parse().ok()?;
            let mut words = Vec::new();
            if number < 0 {
                words.push("minus");
            }
            number_words(number.unsigned_abs(), &mut words);
            for word in words {
                out.push(ASSETS.iter().position(|a| !a.system && a.text == word)?);
            }
            rest = &rest[len..];
        } else if rest.starts_with("cr")
            && rest
                .as_bytes()
                .get(2)
                .is_none_or(|c| !c.is_ascii_alphanumeric())
        {
            out.push(
                ASSETS
                    .iter()
                    .position(|a| !a.system && a.text == "credits")?,
            );
            rest = &rest[2..];
        } else if rest.starts_with(['.', ',', ':', '-']) {
            rest = &rest[1..];
        } else {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_file_is_supported_and_unknown_text_is_silent() {
        for a in ASSETS {
            assert!(decode(a.wav).is_some(), "{}", a.text);
        }
        assert!(decode(b"not a WAV").is_none());
        assert!(plan("Unknown future dialogue", false).is_none());
    }

    #[test]
    fn dynamic_numbers_and_nested_payment_keep_the_exact_values() {
        for text in [
            "Charge seated. 2 to go. Steady hands, kiddo.",
            "Arrested. 5-star fine of 1,500 cr: your cargo covered 400 cr and 1,100 cr came out of your earnings. Your record is clean. My nerves are not.",
            "That's \"Rust Queen\" Ivanna's crew. Bounty of 1,400 cr on them, and they're coming for you, not the cargo.",
        ] { assert!(plan(text, false).is_some(), "{text}"); }
        let mut words = Vec::new();
        number_words(1_500, &mut words);
        assert_eq!(words, ["one", "thousand", "five", "hundred"]);
    }

    #[test]
    fn playback_resamples_and_stop_cancels_immediately() {
        let generation = Arc::new(AtomicU64::new(1));
        let mut p = Playback::new(vec![0.0, 1.0], generation.clone(), 1);
        assert_eq!(p.sample(48_000.0), Some(0.0));
        assert_eq!(p.sample(48_000.0), Some(0.5));
        generation.store(2, Ordering::Relaxed);
        assert_eq!(p.sample(48_000.0), None);
    }
}

include!("recording_coverage.rs");
