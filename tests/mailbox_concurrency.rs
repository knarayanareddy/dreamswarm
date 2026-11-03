use dreamswarm::swarm::mailbox::Mailbox;
use std::sync::{Arc, Barrier};

#[test]
fn concurrent_sends_and_receives_preserve_every_message() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    let mut receiver = Mailbox::new(base.clone(), "team", "worker").unwrap();
    let barrier = Arc::new(Barrier::new(5));
    let mut threads = Vec::new();
    for sender in 0..4 {
        let base = base.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let mailbox = Mailbox::new(base, "team", &format!("sender{sender}")).unwrap();
            barrier.wait();
            for index in 0..100 {
                mailbox
                    .send_chat("worker", &format!("{sender}-{index}"))
                    .unwrap();
            }
        }));
    }
    barrier.wait();
    let mut received = Vec::new();
    while threads.iter().any(|thread| !thread.is_finished()) {
        received.extend(receiver.receive().unwrap());
        std::thread::yield_now();
    }
    for thread in threads {
        thread.join().unwrap();
    }
    received.extend(receiver.receive().unwrap());
    let unique: std::collections::HashSet<_> = received
        .iter()
        .map(|m| match &m.content {
            dreamswarm::swarm::MessageContent::Chat { text } => text.clone(),
            _ => panic!("unexpected message"),
        })
        .collect();
    assert_eq!(received.len(), 400);
    assert_eq!(unique.len(), 400);
    assert!(receiver.receive().unwrap().is_empty());
}
