//! The `request_processor` processes thin client Request messages.

use transaction_processor::TransactionProcessor;
use request::{Request, Response};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct RequestProcessor {
    transaction_processor: Arc<TransactionProcessor>,
}

impl RequestProcessor {
    /// Create a new TxCreator that wraps the given TransactionProcessor.
    pub fn new(transaction_processor: Arc<TransactionProcessor>) -> Self {
        RequestProcessor { transaction_processor }
    }

    /// Process Request items sent by clients.
    fn process_request(
        &self,
        msg: Request,
        rsp_addr: SocketAddr,
    ) -> Option<(Response, SocketAddr)> {
        match msg {
            Request::GetAccount { key } => {
                let account = self.transaction_processor.get_account(&key);
                let rsp = (Response::Account { key, account }, rsp_addr);
                info!("Response::Account {:?}", rsp);
                Some(rsp)
            }
            Request::GetLastId => {
                let id = self.transaction_processor.last_id();
                let rsp = (Response::LastId { id }, rsp_addr);
                info!("Response::LastId {:?}", rsp);
                Some(rsp)
            }
            Request::GetTransactionCount => {
                let transaction_count = self.transaction_processor.transaction_count() as u64;
                let rsp = (Response::TransactionCount { transaction_count }, rsp_addr);
                info!("Response::TransactionCount {:?}", rsp);
                Some(rsp)
            }
            Request::GetSignature { signature } => {
                let signature_status = self.transaction_processor.has_signature(&signature);
                let rsp = (Response::SignatureStatus { signature_status }, rsp_addr);
                info!("Response::Signature {:?}", rsp);
                Some(rsp)
            }
            Request::GetFinality => {
                let time = self.transaction_processor.finality();
                let rsp = (Response::Finality { time }, rsp_addr);
                info!("Response::Finality {:?}", rsp);
                Some(rsp)
            }
        }
    }

    pub fn process_requests(
        &self,
        reqs: Vec<(Request, SocketAddr)>,
    ) -> Vec<(Response, SocketAddr)> {
        reqs.into_iter()
            .filter_map(|(req, rsp_addr)| self.process_request(req, rsp_addr))
            .collect()
    }
}
