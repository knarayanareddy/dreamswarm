// tests/daemon_integration.rs
//! Integration tests for the KAIROS background daemon components.

use dreamswarm::daemon::trust::TrustSystem;
use tempfile::TempDir;

#[test]
fn test_trust_degradation_and_recovery() {
    let tmp = TempDir::new().unwrap();
    let mut trust = TrustSystem::new(tmp.path().to_path_buf()).unwrap();

    let initial = trust.level();
    assert!(initial > 0.5, "Initial trust should be reasonable");

    // Three consecutive denials should degrade trust
    trust.record_denial();
    trust.record_denial();
    trust.record_denial();

    assert!(trust.level() < initial, "Trust should degrade after denials");

    // Approvals should slowly restore trust
    for _ in 0..10 {
        trust.record_approval();
    }

    assert!(trust.level() > trust.level_before_approvals());
}

#[test]
fn test_trust_pause_threshold() {
    let tmp = TempDir::new().unwrap();
    let mut trust = TrustSystem::new(tmp.path().to_path_buf()).unwrap();

    // Aggressively degrade trust
    for _ in 0..20 {
        trust.record_denial();
    }

    assert!(
        trust.is_paused(),
        "Daemon should pause when trust falls below threshold"
    );
}
