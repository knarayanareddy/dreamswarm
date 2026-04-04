// tests/daemon_integration.rs
//! Integration tests for the KAIROS background daemon components.

use dreamswarm::daemon::trust::TrustSystem;

#[test]
fn test_trust_degradation_and_recovery() {
    let mut trust = TrustSystem::new();

    let initial = trust.current_level;
    assert!(initial > 0.5, "Initial trust should be reasonable");

    // Three consecutive denials should degrade trust
    trust.record_denial("test denial 1");
    trust.record_denial("test denial 2");
    trust.record_denial("test denial 3");

    assert!(trust.current_level < initial, "Trust should degrade after denials");

    let after_denials = trust.current_level;

    // Approvals should slowly restore trust
    for i in 0..10 {
        trust.record_approval(&format!("test approval {}", i));
    }

    assert!(
        trust.current_level > after_denials,
        "Trust should recover after approvals"
    );
}

#[test]
fn test_trust_pause_threshold() {
    let mut trust = TrustSystem::new();

    // Aggressively degrade trust
    for i in 0..20 {
        trust.record_denial(&format!("denial {}", i));
    }

    assert!(
        trust.is_paused(),
        "Daemon should pause when trust falls below threshold"
    );
}
