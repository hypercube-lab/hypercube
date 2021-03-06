use transaction_processor::TransactionProcessor;
use bincode::deserialize;
use fin_plan_transaction::FinPlanTransaction;
use counter::Counter;
use entry::Entry;
use log::Level;
use packet::Packets;
use pod_recorder::PodRecorder;
use rayon::prelude::*;
use result::{Error, Result};
use service::Service;
use sigverify_stage::VerifiedPackets;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::thread::{self, Builder, JoinHandle};
use std::time::Duration;
use std::time::Instant;
use timing;
use transaction::Transaction;

pub const NUM_THREADS: usize = 1;

pub struct TransactionProcessoringStage {
    thread_hdls: Vec<JoinHandle<()>>,
}

pub enum Config {
    Tick(usize),
    Sleep(Duration),
}

impl Default for Config {
    fn default() -> Config {
        Config::Sleep(Duration::from_millis(500))
    }
}
impl TransactionProcessoringStage {
    pub fn new(
        transaction_processor: &Arc<TransactionProcessor>,
        verified_receiver: Receiver<VerifiedPackets>,
        config: Config,
    ) -> (Self, Receiver<Vec<Entry>>) {
        let (entry_sender, entry_receiver) = channel();
        let shared_verified_receiver = Arc::new(Mutex::new(verified_receiver));
        let pod = PodRecorder::new(transaction_processor.clone(), entry_sender);
        let tick_pod = pod.clone();
        let pod_exit = Arc::new(AtomicBool::new(false));
        let transaction_processoring_exit = pod_exit.clone();
        let tick_producer = Builder::new()
            .name("hypercube-transaction_processoring-stage-tick_producer".to_string())
            .spawn(move || {
                if let Err(e) = Self::tick_producer(&tick_pod, &config, &pod_exit) {
                    match e {
                        Error::SendError => (),
                        _ => error!(
                            "hypercube-transaction_processoring-stage-tick_producer unexpected error {:?}",
                            e
                        ),
                    }
                }
                debug!("tick producer exiting");
                pod_exit.store(true, Ordering::Relaxed);
            }).unwrap();

        let mut thread_hdls: Vec<JoinHandle<()>> = (0..NUM_THREADS)
            .into_iter()
            .map(|_| {
                let thread_transaction_processor = transaction_processor.clone();
                let thread_verified_receiver = shared_verified_receiver.clone();
                let thread_pod = pod.clone();
                let thread_transaction_processoring_exit = transaction_processoring_exit.clone();
                Builder::new()
                    .name("hypercube-transaction_processoring-stage-tx".to_string())
                    .spawn(move || {
                        loop {
                            if let Err(e) = Self::process_packets(
                                &thread_transaction_processor,
                                &thread_verified_receiver,
                                &thread_pod,
                            ) {
                                debug!("got error {:?}", e);
                                match e {
                                    Error::RecvTimeoutError(RecvTimeoutError::Timeout) => (),
                                    Error::RecvTimeoutError(RecvTimeoutError::Disconnected) => {
                                        break
                                    }
                                    Error::RecvError(_) => break,
                                    Error::SendError => break,
                                    _ => error!("hypercube-transaction_processoring-stage-tx {:?}", e),
                                }
                            }
                            if thread_transaction_processoring_exit.load(Ordering::Relaxed) {
                                debug!("tick service exited");
                                break;
                            }
                        }
                        thread_transaction_processoring_exit.store(true, Ordering::Relaxed);
                    }).unwrap()
            }).collect();
        thread_hdls.push(tick_producer);
        (TransactionProcessoringStage { thread_hdls }, entry_receiver)
    }


    fn deserialize_transactions(p: &Packets) -> Vec<Option<(Transaction, SocketAddr)>> {
        p.packets
            .par_iter()
            .map(|x| {
                deserialize(&x.data[0..x.meta.size])
                    .map(|req| (req, x.meta.addr()))
                    .ok()
            }).collect()
    }

    fn tick_producer(pod: &PodRecorder, config: &Config, pod_exit: &AtomicBool) -> Result<()> {
        loop {
            match *config {
                Config::Tick(num) => {
                    for _ in 0..num {
                        pod.hash();
                    }
                }
                Config::Sleep(duration) => {
                    sleep(duration);
                }
            }
            pod.tick()?;
            if pod_exit.load(Ordering::Relaxed) {
                debug!("tick service exited");
                return Ok(());
            }
        }
    }

    fn process_transactions(
        transaction_processor: &Arc<TransactionProcessor>,
        transactions: &[Transaction],
        pod: &PodRecorder,
    ) -> Result<()> {
        debug!("transactions: {}", transactions.len());
        let mut chunk_start = 0;
        while chunk_start != transactions.len() {
            let chunk_end = chunk_start + Entry::num_will_fit(&transactions[chunk_start..]);

            let results = transaction_processor.process_transactions(&transactions[chunk_start..chunk_end]);

            let processed_transactions: Vec<_> = transactions[chunk_start..chunk_end]
                .into_iter()
                .enumerate()
                .filter_map(|(i, x)| match results[i] {
                    Ok(_) => Some(x.clone()),
                    Err(ref e) => {
                        debug!("process transaction failed {:?}", e);
                        None
                    }
                }).collect();

            if !processed_transactions.is_empty() {
                let hash = Transaction::hash(&processed_transactions);
                debug!("processed ok: {} {}", processed_transactions.len(), hash);
                pod.record(hash, processed_transactions)?;
            }
            chunk_start = chunk_end;
        }
        debug!("done process_transactions");
        Ok(())
    }


    pub fn process_packets(
        transaction_processor: &Arc<TransactionProcessor>,
        verified_receiver: &Arc<Mutex<Receiver<VerifiedPackets>>>,
        pod: &PodRecorder,
    ) -> Result<()> {
        let recv_start = Instant::now();
        let mms = verified_receiver
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_millis(100))?;
        let mut reqs_len = 0;
        let mms_len = mms.len();
        info!(
            "@{:?} process start stalled for: {:?}ms batches: {}",
            timing::timestamp(),
            timing::duration_as_ms(&recv_start.elapsed()),
            mms.len(),
        );
        inc_new_counter_info!("transaction_processoring_stage-entries_received", mms_len);
        let transaction_processor_starting_tx_count = transaction_processor.transaction_count();
        let count = mms.iter().map(|x| x.1.len()).sum();
        let proc_start = Instant::now();
        for (msgs, vers) in mms {
            let transactions = Self::deserialize_transactions(&msgs.read().unwrap());
            reqs_len += transactions.len();

            debug!("transactions received {}", transactions.len());

            let transactions: Vec<_> = transactions
                .into_iter()
                .zip(vers)
                .filter_map(|(tx, ver)| match tx {
                    None => None,
                    Some((tx, _addr)) => if tx.verify_plan() && ver != 0 {
                        Some(tx)
                    } else {
                        None
                    },
                }).collect();
            debug!("verified transactions {}", transactions.len());
            Self::process_transactions(transaction_processor, &transactions, pod)?;
        }

        inc_new_counter_info!(
            "transaction_processoring_stage-time_ms",
            timing::duration_as_ms(&proc_start.elapsed()) as usize
        );
        let total_time_s = timing::duration_as_s(&proc_start.elapsed());
        let total_time_ms = timing::duration_as_ms(&proc_start.elapsed());
        info!(
            "@{:?} done processing transaction batches: {} time: {:?}ms reqs: {} reqs/s: {}",
            timing::timestamp(),
            mms_len,
            total_time_ms,
            reqs_len,
            (reqs_len as f32) / (total_time_s)
        );
        inc_new_counter_info!("transaction_processoring_stage-process_packets", count);
        inc_new_counter_info!(
            "transaction_processoring_stage-process_transactions",
            transaction_processor.transaction_count() - transaction_processor_starting_tx_count
        );
        Ok(())
    }
}

impl Service for TransactionProcessoringStage {
    type JoinReturnType = ();

    fn join(self) -> thread::Result<()> {
        for thread_hdl in self.thread_hdls {
            thread_hdl.join()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transaction_processor::TransactionProcessor;
    use ledger::Block;
    use mint::Mint;
    use packet::to_packets;
    use signature::{Keypair, KeypairUtil};
    use std::thread::sleep;
    use builtin_tansaction::SystemTransaction;
    use transaction::Transaction;

    #[test]
    fn test_transaction_processoring_stage_shutdown1() {
        let transaction_processor = TransactionProcessor::new(&Mint::new(2));
        let (verified_sender, verified_receiver) = channel();
        let (transaction_processoring_stage, _entry_receiver) =
            TransactionProcessoringStage::new(&Arc::new(transaction_processor), verified_receiver, Default::default());
        drop(verified_sender);
        assert_eq!(transaction_processoring_stage.join().unwrap(), ());
    }

    #[test]
    fn test_transaction_processoring_stage_shutdown2() {
        let transaction_processor = TransactionProcessor::new(&Mint::new(2));
        let (_verified_sender, verified_receiver) = channel();
        let (transaction_processoring_stage, entry_receiver) =
            TransactionProcessoringStage::new(&Arc::new(transaction_processor), verified_receiver, Default::default());
        drop(entry_receiver);
        assert_eq!(transaction_processoring_stage.join().unwrap(), ());
    }

    #[test]
    fn test_transaction_processoring_stage_tick() {
        let transaction_processor = Arc::new(TransactionProcessor::new(&Mint::new(2)));
        let start_hash = transaction_processor.last_id();
        let (verified_sender, verified_receiver) = channel();
        let (transaction_processoring_stage, entry_receiver) = TransactionProcessoringStage::new(
            &transaction_processor,
            verified_receiver,
            Config::Sleep(Duration::from_millis(1)),
        );
        sleep(Duration::from_millis(500));
        drop(verified_sender);

        let entries: Vec<_> = entry_receiver.iter().flat_map(|x| x).collect();
        assert!(entries.len() != 0);
        assert!(entries.verify(&start_hash));
        assert_eq!(entries[entries.len() - 1].id, transaction_processor.last_id());
        assert_eq!(transaction_processoring_stage.join().unwrap(), ());
    }

    #[test]
    fn test_transaction_processoring_stage_entries_only() {
        let mint = Mint::new(2);
        let transaction_processor = Arc::new(TransactionProcessor::new(&mint));
        let start_hash = transaction_processor.last_id();
        let (verified_sender, verified_receiver) = channel();
        let (transaction_processoring_stage, entry_receiver) =
            TransactionProcessoringStage::new(&transaction_processor, verified_receiver, Default::default());

        let keypair = mint.keypair();
        let tx = Transaction::system_new(&keypair, keypair.pubkey(), 1, start_hash);

        let tx_no_ver = Transaction::system_new(&keypair, keypair.pubkey(), 1, start_hash);

        let keypair = Keypair::new();
        let tx_anf = Transaction::system_new(&keypair, keypair.pubkey(), 1, start_hash);

        let packets = to_packets(&[tx, tx_no_ver, tx_anf]);

        assert_eq!(packets.len(), 1);
        verified_sender                       // tx, no_ver, anf
            .send(vec![(packets[0].clone(), vec![1u8, 0u8, 1u8])])
            .unwrap();

        drop(verified_sender);

        let entries: Vec<_> = entry_receiver.iter().map(|x| x).collect();
        assert!(entries.len() >= 1);

        let mut last_id = start_hash;
        entries.iter().for_each(|entries| {
            assert_eq!(entries.len(), 1);
            assert!(entries.verify(&last_id));
            last_id = entries.last().unwrap().id;
        });
        drop(entry_receiver);
        assert_eq!(transaction_processoring_stage.join().unwrap(), ());
    }
    #[test]
    fn test_transaction_processoring_stage_entryfication() {
        let mint = Mint::new(2);
        let transaction_processor = Arc::new(TransactionProcessor::new(&mint));
        let (verified_sender, verified_receiver) = channel();
        let (transaction_processoring_stage, entry_receiver) =
            TransactionProcessoringStage::new(&transaction_processor, verified_receiver, Default::default());

        let alice = Keypair::new();
        let tx = Transaction::system_new(&mint.keypair(), alice.pubkey(), 2, mint.last_id());

        let packets = to_packets(&[tx]);
        verified_sender
            .send(vec![(packets[0].clone(), vec![1u8])])
            .unwrap();

        let tx = Transaction::system_new(&alice, mint.pubkey(), 1, mint.last_id());
        let packets = to_packets(&[tx]);
        verified_sender
            .send(vec![(packets[0].clone(), vec![1u8])])
            .unwrap();
        drop(verified_sender);
        assert_eq!(transaction_processoring_stage.join().unwrap(), ());

        let entries: Vec<_> = entry_receiver.iter().flat_map(|x| x).collect();

        assert!(entries.len() >= 2);


        let transaction_processor = TransactionProcessor::new(&mint);
        for entry in entries {
            assert!(
                transaction_processor.process_transactions(&entry.transactions)
                    .into_iter()
                    .all(|x| x.is_ok())
            );
        }
        assert_eq!(transaction_processor.get_balance(&alice.pubkey()), 1);
    }
}
