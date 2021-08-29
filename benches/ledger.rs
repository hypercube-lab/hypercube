#![feature(test)]
extern crate hypercube;
extern crate test;

use hypercube::hash::{hash, Hash};
use hypercube::ledger::{next_entries, reconstruct_entries_from_blobs, Block};
use hypercube::signature::{Keypair, KeypairUtil};
use hypercube::builtin_tansaction::SystemTransaction;
use hypercube::transaction::Transaction;
use test::Bencher;

#[bench]
fn bench_block_to_blobs_to_block(bencher: &mut Bencher) {
    let zero = Hash::default();
    let one = hash(&zero.as_ref());
    let keypair = Keypair::new();
    let tx0 = Transaction::system_move(&keypair, keypair.pubkey(), 1, one, 0);
    let transactions = vec![tx0; 10];
    let entries = next_entries(&zero, 1, transactions);

    bencher.iter(|| {
        let blobs = entries.to_blobs();
        assert_eq!(reconstruct_entries_from_blobs(blobs).unwrap(), entries);
    });
}
