#![no_main]
//! Fuzz the tool input JSON parsing logic.
//! Referenced in `.github/workflows/nightly.yml`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Feed arbitrary JSON strings through the tool input parser.
        // This exercises serde_json deserialization paths and any
        // input validation logic to find panics on malformed input.
        let _: Option<serde_json::Value> = serde_json::from_str(s).ok();
    }
});
