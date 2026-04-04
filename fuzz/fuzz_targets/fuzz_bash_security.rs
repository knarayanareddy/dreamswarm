#![no_main]
//! Fuzz the bash security validator chain.
//! Referenced in `.github/workflows/nightly.yml`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(cmd) = std::str::from_utf8(data) {
        // Feed arbitrary command strings through the bash validator.
        // This exercises the deny-list, pattern matching, and risk-scoring
        // logic to find panics or unexpected allow/deny decisions.
        let _ = dreamswarm::tools::bash_validator::validate(cmd);
    }
});
