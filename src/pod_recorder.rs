//! The `pod_recorder` module provides an object for synchronizing with Proof of Dedication.
//! It synchronizes PoH, transaction_processor's register_entry_id and the ledger
//!
use transaction_processor::TransactionProcessor;
use entry::Entry;
use hash::Hash;
use pod::Pod;
use result::Result;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use transaction::Transaction;

#[derive(Clone)]
pub struct PodRecorder {
    pod: Arc<Mutex<Pod>>,
    transaction_processor: Arc<TransactionProcessor>,
    sender: Sender<Vec<Entry>>,
}

impl PodRecorder {
 
    pub fn new(transaction_processor: Arc<TransactionProcessor>, sender: Sender<Vec<Entry>>) -> Self {
        let pod = Arc::new(Mutex::new(Pod::new(transaction_processor.last_id())));
        PodRecorder { pod, transaction_processor, sender }
    }

    pub fn hash(&self) {
 
        let mut pod = self.pod.lock().unwrap();
        pod.hash()
    }

    pub fn tick(&self) -> Result<()> {
 
        let mut pod = self.pod.lock().unwrap();
        let tick = pod.tick();
        self.transaction_processor.register_entry_id(&tick.id);
        let entry = Entry {
            num_hashes: tick.num_hashes,
            id: tick.id,
            transactions: vec![],
        };
        self.sender.send(vec![entry])?;
        Ok(())
    }

    pub fn record(&self, mixin: Hash, txs: Vec<Transaction>) -> Result<()> {
 
        let mut pod = self.pod.lock().unwrap();
        let tick = pod.record(mixin);
        self.transaction_processor.register_entry_id(&tick.id);
        let entry = Entry {
            num_hashes: tick.num_hashes,
            id: tick.id,
            transactions: txs,
        };
        self.sender.send(vec![entry])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hash::hash;
    use mint::Mint;
    use std::sync::mpsc::channel;
    use std::sync::Arc;

    #[test]
    fn test_pod() {
        let mint = Mint::new(1);
        let transaction_processor = Arc::new(TransactionProcessor::new(&mint));
        let (entry_sender, entry_receiver) = channel();
        let pod_recorder = PodRecorder::new(transaction_processor, entry_sender);

 
        let h1 = hash(b"hello world!");
        assert!(pod_recorder.record(h1, vec![]).is_ok());
        assert!(pod_recorder.tick().is_ok());

 
        let _ = entry_receiver.recv().unwrap();
        let _ = entry_receiver.recv().unwrap();

 
        drop(entry_receiver);
        assert!(pod_recorder.tick().is_err());
    }
}
