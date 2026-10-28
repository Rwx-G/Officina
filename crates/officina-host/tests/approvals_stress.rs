//! Single use without a lock (AD-33, AD-37 stress): an approval is used by
//! claiming `<id>.consumed.json` with `create_new`, and of two runs that
//! claim it at once exactly one wins. A rename is not such a claim on
//! Windows (story 2.8's Debug Log), which this test caught.
//! Ignored by default: CI runs it in release with
//! `cargo test --release -p officina-host --test approvals_stress -- --ignored`.

use std::sync::Barrier;

use officina_core::relay::approvals::{FileKind, file_name};
use officina_host::relay::approvals::consume;
use officina_host::relay::home::RelayHome;

const ROUNDS: usize = 500;

#[test]
#[ignore = "stress test, run in release by CI"]
fn two_consumers_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let approvals = home.approvals_dir();
    std::fs::create_dir_all(&approvals).unwrap();
    let barrier = Barrier::new(2);
    for round in 0..ROUNDS {
        let id = format!("0123456789abcdef-{round}");
        std::fs::write(approvals.join(file_name(&id, FileKind::Approved)), b"{}").unwrap();
        let wins: usize = std::thread::scope(|scope| {
            let consumers: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        consume(&home, &id).is_ok()
                    })
                })
                .collect();
            consumers
                .into_iter()
                .map(|c| usize::from(c.join().unwrap()))
                .sum()
        });
        assert_eq!(wins, 1, "round {round}");
        assert!(approvals.join(file_name(&id, FileKind::Consumed)).is_file());
        assert!(!approvals.join(file_name(&id, FileKind::Approved)).exists());
    }
}
