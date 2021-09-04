use transaction_processor::TransactionProcessor;
use request_processor::RequestProcessor;
use request_stage::RequestStage;
use service::Service;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use streamer;

pub struct Rpu {
    request_stage: RequestStage,
    thread_hdls: Vec<JoinHandle<()>>,
    exit: Arc<AtomicBool>,
}

impl Rpu {
    pub fn new(transaction_processor: &Arc<TransactionProcessor>, requests_socket: UdpSocket, respond_socket: UdpSocket) -> Self {
        let exit = Arc::new(AtomicBool::new(false));
        let (packet_sender, packet_receiver) = channel();
        let t_receiver = streamer::receiver(
            Arc::new(requests_socket),
            exit.clone(),
            packet_sender,
            "rpu",
        );

        let request_processor = RequestProcessor::new(transaction_processor.clone());
        let (request_stage, blob_receiver) = RequestStage::new(request_processor, packet_receiver);

        let t_responder = streamer::responder("rpu", Arc::new(respond_socket), blob_receiver);

        let thread_hdls = vec![t_receiver, t_responder];

        Rpu {
            thread_hdls,
            request_stage,
            exit,
        }
    }

    pub fn exit(&self) {
        self.exit.store(true, Ordering::Relaxed);
    }

    pub fn close(self) -> thread::Result<()> {
        self.exit();
        self.join()
    }
}

impl Service for Rpu {
    type JoinReturnType = ();

    fn join(self) -> thread::Result<()> {
        for thread_hdl in self.thread_hdls {
            thread_hdl.join()?;
        }
        self.request_stage.join()?;
        Ok(())
    }
}
