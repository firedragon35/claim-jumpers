# Claim Jumpers voice pack

260 locally generated neural voice WAVs, based on `firedragon35/claim-jumpers-native` at commit `bcfcc970818775c37093259a2713fc63eed85c2c` (main when inspected on November 12, 2026).

- Marge: Kokoro `bf_emma`, British English, speed 0.92.
- Cockpit warning Dispatcher: Kokoro `af_heart`, American English, speed 1.02.
- Format: 24,000 Hz, mono, signed 16-bit PCM WAV.
- No paid service was used. These are finished sound files; players need no model, installed speech voice, internet connection, or credits.

Compatibility revalidated against main at `14857a7b2e48a9e00b8309e7df1ac3d1c0657e6a` (version 1.9.0). The dialogue source files are unchanged from the recording inventory; all 260 audio hashes are unchanged. The refreshed integration proposes 1.10.0.

## Contents

| Recordings | Count |
| --- | ---: |
| Marge complete lines | 133 |
| Cockpit warning complete lines | 28 |
| Marge asides | 8 |
| Dynamic phrase fragments | 33 |
| Boss names, pod contents, rare salvage labels | 20 |
| Number words and credits | 38 |
| Total WAVs | 260 |

`manifest.json` maps every file to its exact text, character, event aliases, source file and line, immutable GitHub source link, duration and SHA-256. `lines.csv` is a spreadsheet-friendly line list. Filenames are stable event-based names, for example `marge/line/start_01.wav`, `dispatcher/line/hulllow.wav`, and `marge/aside/aside_01.wav`.

## Exact lines and changing dialogue

The source text is preserved in `text`. `spoken_text` records the actual synthesis input: double quotation marks are removed and the credit abbreviation `cr` is spoken as “credits”, matching the game's previous speech normalization. The wording is not paraphrased. The sound-test “I am almost impressed” line is kept separately from gameplay's “I'm almost impressed” template.

The inventory includes the stock radio table, cockpit callouts, asides, five tutorial steps and both endings, co-op messages, radiation and refinery notices, adaptation intel, and sound-test speech. Shop text that never reaches speech is outside this pack's spoken inventory.

Eleven top-level live templates use changing counts, credit amounts, names or labels. Arrest payment wording has two additional nested numeric templates and two fixed alternatives. Their original Rust format strings, canonical named slots and exact fragment order are in `dynamic_templates`; finite name/content/label choices are in `slot_values`. Recording every possible credit amount as a full line is not finite. The prepared integration assembles those lines from the recorded phrases and number words, preserving the changing values without runtime TTS.

## Validation

Every WAV was decoded and checked for the required format, finite non-silent samples and a matching SHA-256. The user approved previews of the two voices. The complete 13-minute pack has not received a line-by-line listening or transcription review. Short asides and the joins in changing dialogue may need listening-based tuning.

The accompanying source integration passed formatting, strict Clippy, all 369 workspace tests (one existing test ignored), and the twelve-screen debug smoke tour. The smoke tour disables speech by design; it verified the audio device and bundled voice loading, not an audible full-pack playthrough. Unit tests cover routing, live-value examples, resampling, interruption, voice-off and mute.

## Voice provenance

Generated locally with Kokoro-82M v1.0 using the full-precision ONNX model. Model and voice-file hashes are in the manifest. The speech model is published under Apache 2.0; the kokoro-onnx inference library uses MIT. The model and inference software are not distributed in this pack.

- Model: https://huggingface.co/hexgrad/Kokoro-82M
- Voices: https://huggingface.co/hexgrad/Kokoro-82M/blob/main/VOICES.md
- Inference library and model downloads: https://github.com/thewh1teagle/kokoro-onnx

These are synthetic preset voices, not recordings of hired actors or clones requested from a real person. Original game dialogue ownership is unchanged.
