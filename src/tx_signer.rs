use transaction_processor::TransactionProcessor;
use blob_fetch_stage::BlobFetchStage;
use blockthread::BlockThread;
use replicate_stage::ReplicateStage;
use retransmit_stage::{RetransmitStage, RetransmitStageReturnType};
use service::Service;
use signature::Keypair;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use window::SharedWindow;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TxSignerReturnType {
    LeaderRotation(u64),
}

pub struct TxSigner {
    replicate_stage: ReplicateStage,
    fetch_stage: BlobFetchStage,
    retransmit_stage: RetransmitStage,
    exit: Arc<AtomicBool>,
}

impl TxSigner {
    #[cfg_attr(feature = "cargo-clippy", allow(too_many_arguments))]
    pub fn new(
        keypair: Arc<Keypair>,
        transaction_processor: &Arc<TransactionProcessor>,
        entry_height: u64,
        blockthread: Arc<RwLock<BlockThread>>,
        window: SharedWindow,
        replicate_sockets: Vec<UdpSocket>,
        repair_socket: UdpSocket,
        retransmit_socket: UdpSocket,
        ledger_path: Option<&str>,
    ) -> Self {
        let exit = Arc::new(AtomicBool::new(false));

        let repair_socket = Arc::new(repair_socket);
        let mut blob_sockets: Vec<Arc<UdpSocket>> =
            replicate_sockets.into_iter().map(Arc::new).collect();
        blob_sockets.push(repair_socket.clone());
        let (fetch_stage, blob_fetch_receiver) =
            BlobFetchStage::new_multi_socket(blob_sockets, exit.clone());
        let (retransmit_stage, blob_window_receiver) = RetransmitStage::new(
            &blockthread,
            window,
            entry_height,
            Arc::new(retransmit_socket),
            repair_socket,
            blob_fetch_receiver,
        );

        let replicate_stage = ReplicateStage::new(
            keypair,
            transaction_processor.clone(),
            blockthread,
            blob_window_receiver,
            ledger_path,
            exit.clone(),
        );

        TxSigner {
            replicate_stage,
            fetch_stage,
            retransmit_stage,
            exit,
        }
    }

    pub fn exit(&self) -> () {
        self.exit.store(true, Ordering::Relaxed);
    }

    pub fn close(self) -> thread::Result<Option<TxSignerReturnType>> {
        self.fetch_stage.close();
        self.join()
    }
}

impl Service for TxSigner {
    type JoinReturnType = Option<TxSignerReturnType>;

    fn join(self) -> thread::Result<Option<TxSignerReturnType>> {
        self.replicate_stage.join()?;
        self.fetch_stage.join()?;
        match self.retransmit_stage.join()? {
            Some(RetransmitStageReturnType::LeaderRotation(entry_height)) => {
                Ok(Some(TxSignerReturnType::LeaderRotation(entry_height)))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use transaction_processor::TransactionProcessor;
    use bincode::serialize;
    use blockthread::{BlockThread, Node};
    use entry::Entry;
    use hash::{hash, Hash};
    use logger;
    use mint::Mint;
    use ncp::Ncp;
    use packet::SharedBlob;
    use service::Service;
    use signature::{Keypair, KeypairUtil};
    use std::net::UdpSocket;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use streamer;
    use builtin_tansaction::SystemTransaction;
    use transaction::Transaction;
    use tx_signer::TxSigner;
    use window::{self, SharedWindow};

    fn new_ncp(
        blockthread: Arc<RwLock<BlockThread>>,
        gossip: UdpSocket,
        exit: Arc<AtomicBool>,
    ) -> (Ncp, SharedWindow) {
        let window = Arc::new(RwLock::new(window::default_window()));
        let ncp = Ncp::new(&blockthread, window.clone(), None, gossip, exit);
        (ncp, window)
    }

    #[test]
    fn test_replicate() {
        logger::setup();
        let leader = Node::new_localhost();
        let target1_keypair = Keypair::new();
        let target1 = Node::new_localhost_with_pubkey(target1_keypair.pubkey());
        let target2 = Node::new_localhost();
        let exit = Arc::new(AtomicBool::new(false));

        let mut blockthread_l = BlockThread::new(leader.info.clone()).expect("BlockThread::new");
        blockthread_l.set_leader(leader.info.id);

        let cref_l = Arc::new(RwLock::new(blockthread_l));
        let dr_l = new_ncp(cref_l, leader.sockets.gossip, exit.clone());

        let mut blockthread2 = BlockThread::new(target2.info.clone()).expect("BlockThread::new");
        blockthread2.insert(&leader.info);
        blockthread2.set_leader(leader.info.id);
        let leader_id = leader.info.id;
        let cref2 = Arc::new(RwLock::new(blockthread2));
        let dr_2 = new_ncp(cref2, target2.sockets.gossip, exit.clone());
        let (s_reader, r_reader) = channel();
        let blob_sockets: Vec<Arc<UdpSocket>> = target2
            .sockets
            .replicate
            .into_iter()
            .map(Arc::new)
            .collect();

        let t_receiver = streamer::blob_receiver(blob_sockets[0].clone(), exit.clone(), s_reader);

        let (s_responder, r_responder) = channel();
        let t_responder = streamer::responder(
            "test_replicate",
            Arc::new(leader.sockets.requests),
            r_responder,
        );

        let starting_balance = 10_000;
        let mint = Mint::new(starting_balance);
        let replicate_addr = target1.info.contact_info.tx_signer;
        let transaction_processor = Arc::new(TransactionProcessor::new(&mint));

        let mut blockthread1 = BlockThread::new(target1.info.clone()).expect("BlockThread::new");
        blockthread1.insert(&leader.info);
        blockthread1.set_leader(leader.info.id);
        let cref1 = Arc::new(RwLock::new(blockthread1));
        let dr_1 = new_ncp(cref1.clone(), target1.sockets.gossip, exit.clone());

        let tx_signer = TxSigner::new(
            Arc::new(target1_keypair),
            &transaction_processor,
            0,
            cref1,
            dr_1.1,
            target1.sockets.replicate,
            target1.sockets.repair,
            target1.sockets.retransmit,
            None,
        );

        let mut alice_ref_balance = starting_balance;
        let mut msgs = Vec::new();
        let mut cur_hash = Hash::default();
        let mut blob_id = 0;
        let num_transfers = 10;
        let transfer_amount = 501;
        let bob_keypair = Keypair::new();
        for i in 0..num_transfers {
            let entry0 = Entry::new(&cur_hash, i, vec![]);
            transaction_processor.register_entry_id(&cur_hash);
            cur_hash = hash(&cur_hash.as_ref());

            let tx0 = Transaction::system_new(
                &mint.keypair(),
                bob_keypair.pubkey(),
                transfer_amount,
                cur_hash,
            );
            transaction_processor.register_entry_id(&cur_hash);
            cur_hash = hash(&cur_hash.as_ref());
            let entry1 = Entry::new(&cur_hash, i + num_transfers, vec![tx0]);
            transaction_processor.register_entry_id(&cur_hash);
            cur_hash = hash(&cur_hash.as_ref());

            alice_ref_balance -= transfer_amount;

            for entry in vec![entry0, entry1] {
                let mut b = SharedBlob::default();
                {
                    let mut w = b.write().unwrap();
                    w.set_index(blob_id).unwrap();
                    blob_id += 1;
                    w.set_id(leader_id).unwrap();

                    let serialized_entry = serialize(&entry).unwrap();

                    w.data_mut()[..serialized_entry.len()].copy_from_slice(&serialized_entry);
                    w.set_size(serialized_entry.len());
                    w.meta.set_addr(&replicate_addr);
                }
                msgs.push(b);
            }
        }

        s_responder.send(msgs).expect("send");
        drop(s_responder);

        let timer = Duration::new(1, 0);
        while let Ok(_msg) = r_reader.recv_timeout(timer) {
            trace!("got msg");
        }

        let alice_balance = transaction_processor.get_balance(&mint.keypair().pubkey());
        assert_eq!(alice_balance, alice_ref_balance);

        let bob_balance = transaction_processor.get_balance(&bob_keypair.pubkey());
        assert_eq!(bob_balance, starting_balance - alice_ref_balance);

        tx_signer.close().expect("close");
        exit.store(true, Ordering::Relaxed);
        dr_l.0.join().expect("join");
        dr_2.0.join().expect("join");
        dr_1.0.join().expect("join");
        t_receiver.join().expect("join");
        t_responder.join().expect("join");
    }
}
