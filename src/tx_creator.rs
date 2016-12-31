use transaction_processor::TransactionProcessor;
use transaction_processoring_stage::{TransactionProcessoringStage, Config};
use blockthread::BlockThread;
use entry::Entry;
use fetch_stage::FetchStage;
use service::Service;
use signature::Keypair;
use sigverify_stage::SigVerifyStage;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::thread;
use write_stage::{WriteStage, WriteStageReturnType};

pub enum TxCreatorReturnType {
    LeaderRotation,
}

pub struct TxCreator {
    fetch_stage: FetchStage,
    sigverify_stage: SigVerifyStage,
    transaction_processoring_stage: TransactionProcessoringStage,
    write_stage: WriteStage,
    exit: Arc<AtomicBool>,
}

impl TxCreator {
    pub fn new(
        keypair: Arc<Keypair>,
        transaction_processor: &Arc<TransactionProcessor>,
        blockthread: &Arc<RwLock<BlockThread>>,
        tick_duration: Config,
        transactions_sockets: Vec<UdpSocket>,
        ledger_path: &str,
        sigverify_disabled: bool,
        entry_height: u64,
    ) -> (Self, Receiver<Vec<Entry>>, Arc<AtomicBool>) {
        let exit = Arc::new(AtomicBool::new(false));

        let (fetch_stage, packet_receiver) = FetchStage::new(transactions_sockets, exit.clone());

        let (sigverify_stage, verified_receiver) =
            SigVerifyStage::new(packet_receiver, sigverify_disabled);

        let (transaction_processoring_stage, entry_receiver) =
            TransactionProcessoringStage::new(&transaction_processor, verified_receiver, tick_duration);

        let (write_stage, entry_forwarder) = WriteStage::new(
            keypair,
            transaction_processor.clone(),
            blockthread.clone(),
            ledger_path,
            entry_receiver,
            entry_height,
        );

        let tx_creator = TxCreator {
            fetch_stage,
            sigverify_stage,
            transaction_processoring_stage,
            write_stage,
            exit: exit.clone(),
        };
        (tx_creator, entry_forwarder, exit)
    }

    pub fn exit(&self) -> () {
        self.exit.store(true, Ordering::Relaxed);
    }

    pub fn close(self) -> thread::Result<Option<TxCreatorReturnType>> {
        self.fetch_stage.close();
        self.join()
    }
}

impl Service for TxCreator {
    type JoinReturnType = Option<TxCreatorReturnType>;

    fn join(self) -> thread::Result<(Option<TxCreatorReturnType>)> {
        self.fetch_stage.join()?;
        self.sigverify_stage.join()?;
        self.transaction_processoring_stage.join()?;
        match self.write_stage.join()? {
            WriteStageReturnType::LeaderRotation => Ok(Some(TxCreatorReturnType::LeaderRotation)),
            _ => Ok(None),
        }
    }
}

