//! The `fullnode` module hosts all the fullnode microservices.

use transaction_processor::TransactionProcessor;
use broadcast_stage::BroadcastStage;
use blockthread::{BlockThread, Node, NodeInfo};
use drone::DRONE_PORT;
use entry::Entry;
use ledger::read_ledger;
use ncp::Ncp;
use rpc::{JsonRpcService, RPC_PORT};
use rpu::Rpu;
use service::Service;
use signature::{Keypair, KeypairUtil};
use xpz_program_interface::pubkey::Pubkey;
use std::net::UdpSocket;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::Result;
use tx_creator::{TxCreator, TxCreatorReturnType};
use tx_signer::{TxSigner, TxSignerReturnType};
use untrusted::Input;
use window;

pub enum NodeRole {
    Leader(LeaderServices),
    Validator(ValidatorServices),
}

pub struct LeaderServices {
    tx_creator: TxCreator,
    broadcast_stage: BroadcastStage,
}

impl LeaderServices {
    fn new(tx_creator: TxCreator, broadcast_stage: BroadcastStage) -> Self {
        LeaderServices {
            tx_creator,
            broadcast_stage,
        }
    }

    pub fn join(self) -> Result<Option<TxCreatorReturnType>> {
        self.broadcast_stage.join()?;
        self.tx_creator.join()
    }

    pub fn exit(&self) -> () {
        self.tx_creator.exit();
    }
}

pub struct ValidatorServices {
    tx_signer: TxSigner,
}

impl ValidatorServices {
    fn new(tx_signer: TxSigner) -> Self {
        ValidatorServices { tx_signer }
    }

    pub fn join(self) -> Result<Option<TxSignerReturnType>> {
        self.tx_signer.join()
    }

    pub fn exit(&self) -> () {
        self.tx_signer.exit()
    }
}

pub enum FullnodeReturnType {
    LeaderRotation,
}

pub struct Fullnode {
    pub node_role: Option<NodeRole>,
    keypair: Arc<Keypair>,
    exit: Arc<AtomicBool>,
    rpu: Option<Rpu>,
    rpc_service: JsonRpcService,
    ncp: Ncp,
    transaction_processor: Arc<TransactionProcessor>,
    blockthread: Arc<RwLock<BlockThread>>,
    ledger_path: String,
    sigverify_disabled: bool,
    shared_window: window::SharedWindow,
    replicate_socket: Vec<UdpSocket>,
    repair_socket: UdpSocket,
    retransmit_socket: UdpSocket,
    transaction_sockets: Vec<UdpSocket>,
    broadcast_socket: UdpSocket,
    requests_socket: UdpSocket,
    respond_socket: UdpSocket,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
/// Fullnode configuration to be stored in file
pub struct Config {
    pub node_info: NodeInfo,
    pkcs8: Vec<u8>,
}

/// Structure to be replicated by the network
impl Config {
    pub fn new(bind_addr: &SocketAddr, pkcs8: Vec<u8>) -> Self {
        let keypair =
            Keypair::from_pkcs8(Input::from(&pkcs8)).expect("from_pkcs8 in fullnode::Config new");
        let pubkey = keypair.pubkey();
        let node_info = NodeInfo::new_with_pubkey_socketaddr(pubkey, bind_addr);
        Config { node_info, pkcs8 }
    }
    pub fn keypair(&self) -> Keypair {
        Keypair::from_pkcs8(Input::from(&self.pkcs8))
            .expect("from_pkcs8 in fullnode::Config keypair")
    }
}

impl Fullnode {
    pub fn new(
        node: Node,
        ledger_path: &str,
        keypair: Keypair,
        leader_addr: Option<SocketAddr>,
        sigverify_disabled: bool,
        leader_rotation_interval: Option<u64>,
    ) -> Self {
        info!("creating transaction_processor...");
        let (transaction_processor, entry_height, ledger_tail) = Self::new_transaction_processor_from_ledger(ledger_path);

        info!("creating networking stack...");
        let local_gossip_addr = node.sockets.gossip.local_addr().unwrap();

        info!(
            "starting... local gossip address: {} (advertising {})",
            local_gossip_addr, node.info.contact_info.ncp
        );

        let local_requests_addr = node.sockets.requests.local_addr().unwrap();
        let requests_addr = node.info.contact_info.rpu;
        let leader_info = leader_addr.map(|i| NodeInfo::new_entry_point(&i));
        let server = Self::new_with_transaction_processor(
            keypair,
            transaction_processor,
            entry_height,
            &ledger_tail,
            node,
            leader_info.as_ref(),
            ledger_path,
            sigverify_disabled,
            leader_rotation_interval,
            None,
        );

        match leader_addr {
            Some(leader_addr) => {
                info!(
                    "validator ready... local request address: {} (advertising {}) connected to: {}",
                    local_requests_addr, requests_addr, leader_addr
                );
            }
            None => {
                info!(
                    "leader ready... local request address: {} (advertising {})",
                    local_requests_addr, requests_addr
                );
            }
        }

        server
    }

    /// Create a fullnode instance acting as a leader or validator.
    ///
    /// ```text
    ///              .---------------------.
    ///              |  Leader             |
    ///              |                     |
    ///  .--------.  |  .-----.            |
    ///  |        |---->|     |            |
    ///  | Client |  |  | RPU |            |
    ///  |        |<----|     |            |
    ///  `----+---`  |  `-----`            |
    ///       |      |     ^               |
    ///       |      |     |               |
    ///       |      |  .--+---.           |
    ///       |      |  | TransactionProcessor |           |
    ///       |      |  `------`           |
    ///       |      |     ^               |
    ///       |      |     |               |    .------------.
    ///       |      |  .--+--.   .-----.  |    |            |
    ///       `-------->| TPU +-->| NCP +------>| Validators |
    ///              |  `-----`   `-----`  |    |            |
    ///              |                     |    `------------`
    ///              `---------------------`
    ///
    ///               .-------------------------------.
    ///               | Validator                     |
    ///               |                               |
    ///   .--------.  |            .-----.            |
    ///   |        |-------------->|     |            |
    ///   | Client |  |            | RPU |            |
    ///   |        |<--------------|     |            |
    ///   `--------`  |            `-----`            |
    ///               |               ^               |
    ///               |               |               |
    ///               |            .--+---.           |
    ///               |            | TransactionProcessor |           |
    ///               |            `------`           |
    ///               |               ^               |
    ///   .--------.  |               |               |    .------------.
    ///   |        |  |            .--+--.            |    |            |
    ///   | Leader |<------------->| TVU +<--------------->|            |
    ///   |        |  |            `-----`            |    | Validators |
    ///   |        |  |               ^               |    |            |
    ///   |        |  |               |               |    |            |
    ///   |        |  |            .--+--.            |    |            |
    ///   |        |<------------->| NCP +<--------------->|            |
    ///   |        |  |            `-----`            |    |            |
    ///   `--------`  |                               |    `------------`
    ///               `-------------------------------`
    /// ```
    #[cfg_attr(feature = "cargo-clippy", allow(too_many_arguments))]
    pub fn new_with_transaction_processor(
        keypair: Keypair,
        transaction_processor: TransactionProcessor,
        entry_height: u64,
        ledger_tail: &[Entry],
        mut node: Node,
        leader_info: Option<&NodeInfo>,
        ledger_path: &str,
        sigverify_disabled: bool,
        leader_rotation_interval: Option<u64>,
        rpc_port: Option<u16>,
    ) -> Self {
        if leader_info.is_none() {
            node.info.leader_id = node.info.id;
        }
        let exit = Arc::new(AtomicBool::new(false));
        let transaction_processor = Arc::new(transaction_processor);

        let rpu = Some(Rpu::new(
            &transaction_processor,
            node.sockets
                .requests
                .try_clone()
                .expect("Failed to clone requests socket"),
            node.sockets
                .respond
                .try_clone()
                .expect("Failed to clone respond socket"),
        ));

        // TODO: this code assumes this node is the leader
        let mut drone_addr = node.info.contact_info.tx_creator;
        drone_addr.set_port(DRONE_PORT);

        // Use custom RPC port, if provided (`Some(port)`)
        // RPC port may be any open port on the node
        // If rpc_port == `None`, node will listen on the default RPC_PORT from Rpc module
        // If rpc_port == `Some(0)`, node will dynamically choose any open port. Useful for tests.
        let rpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from(0)), rpc_port.unwrap_or(RPC_PORT));
        let rpc_service = JsonRpcService::new(
            &transaction_processor,
            node.info.contact_info.tx_creator,
            drone_addr,
            rpc_addr,
            exit.clone(),
        );

        let window = window::new_window_from_entries(ledger_tail, entry_height, &node.info);
        let shared_window = Arc::new(RwLock::new(window));

        let mut blockthread = BlockThread::new(node.info).expect("BlockThread::new");
        if let Some(interval) = leader_rotation_interval {
            blockthread.set_leader_rotation_interval(interval);
        }
        let blockthread = Arc::new(RwLock::new(blockthread));

        let ncp = Ncp::new(
            &blockthread,
            shared_window.clone(),
            Some(ledger_path),
            node.sockets.gossip,
            exit.clone(),
        );

        let keypair = Arc::new(keypair);
        let node_role;
        match leader_info {
            Some(leader_info) => {
                // Start in validator mode.
                // TODO: let BlockThread get that data from the network?
                blockthread.write().unwrap().insert(leader_info);
                let tx_signer = TxSigner::new(
                    keypair.clone(),
                    &transaction_processor,
                    entry_height,
                    blockthread.clone(),
                    shared_window.clone(),
                    node.sockets
                        .replicate
                        .iter()
                        .map(|s| s.try_clone().expect("Failed to clone replicate sockets"))
                        .collect(),
                    node.sockets
                        .repair
                        .try_clone()
                        .expect("Failed to clone repair socket"),
                    node.sockets
                        .retransmit
                        .try_clone()
                        .expect("Failed to clone retransmit socket"),
                    Some(ledger_path),
                );
                let validator_state = ValidatorServices::new(tx_signer);
                node_role = Some(NodeRole::Validator(validator_state));
            }
            None => {
                // Start in leader mode.
                let (tx_creator, entry_receiver, tx_creator_exit) = TxCreator::new(
                    keypair.clone(),
                    &transaction_processor,
                    &blockthread,
                    Default::default(),
                    node.sockets
                        .transaction
                        .iter()
                        .map(|s| s.try_clone().expect("Failed to clone transaction sockets"))
                        .collect(),
                    ledger_path,
                    sigverify_disabled,
                    entry_height,
                );

                let broadcast_stage = BroadcastStage::new(
                    node.sockets
                        .broadcast
                        .try_clone()
                        .expect("Failed to clone broadcast socket"),
                    blockthread.clone(),
                    shared_window.clone(),
                    entry_height,
                    entry_receiver,
                    tx_creator_exit,
                );
                let leader_state = LeaderServices::new(tx_creator, broadcast_stage);
                node_role = Some(NodeRole::Leader(leader_state));
            }
        }

        Fullnode {
            keypair,
            blockthread,
            shared_window,
            transaction_processor,
            sigverify_disabled,
            rpu,
            ncp,
            rpc_service,
            node_role,
            ledger_path: ledger_path.to_owned(),
            exit,
            replicate_socket: node.sockets.replicate,
            repair_socket: node.sockets.repair,
            retransmit_socket: node.sockets.retransmit,
            transaction_sockets: node.sockets.transaction,
            broadcast_socket: node.sockets.broadcast,
            requests_socket: node.sockets.requests,
            respond_socket: node.sockets.respond,
        }
    }

    fn leader_to_validator(&mut self) -> Result<()> {
        // TODO: We can avoid building the transaction_processor again once RecordStage is
        // integrated with TransactionProcessoringStage
        let (transaction_processor, entry_height, _) = Self::new_transaction_processor_from_ledger(&self.ledger_path);
        self.transaction_processor = Arc::new(transaction_processor);

        {
            let mut wblockthread = self.blockthread.write().unwrap();
            let scheduled_leader = wblockthread.get_scheduled_leader(entry_height);
            match scheduled_leader {
                //TODO: Handle the case where we don't know who the next
                //scheduled leader is
                None => (),
                Some(leader_id) => wblockthread.set_leader(leader_id),
            }
        }

        // Make a new RPU to serve requests out of the new transaction_processor we've created
        // instead of the old one
        if self.rpu.is_some() {
            let old_rpu = self.rpu.take().unwrap();
            old_rpu.close()?;
            self.rpu = Some(Rpu::new(
                &self.transaction_processor,
                self.requests_socket
                    .try_clone()
                    .expect("Failed to clone requests socket"),
                self.respond_socket
                    .try_clone()
                    .expect("Failed to clone respond socket"),
            ));
        }

        let tx_signer = TxSigner::new(
            self.keypair.clone(),
            &self.transaction_processor,
            entry_height,
            self.blockthread.clone(),
            self.shared_window.clone(),
            self.replicate_socket
                .iter()
                .map(|s| s.try_clone().expect("Failed to clone replicate sockets"))
                .collect(),
            self.repair_socket
                .try_clone()
                .expect("Failed to clone repair socket"),
            self.retransmit_socket
                .try_clone()
                .expect("Failed to clone retransmit socket"),
            Some(&self.ledger_path),
        );
        let validator_state = ValidatorServices::new(tx_signer);
        self.node_role = Some(NodeRole::Validator(validator_state));
        Ok(())
    }

    fn validator_to_leader(&mut self, entry_height: u64) {
        self.blockthread.write().unwrap().set_leader(self.keypair.pubkey());
        let (tx_creator, blob_receiver, tx_creator_exit) = TxCreator::new(
            self.keypair.clone(),
            &self.transaction_processor,
            &self.blockthread,
            Default::default(),
            self.transaction_sockets
                .iter()
                .map(|s| s.try_clone().expect("Failed to clone transaction sockets"))
                .collect(),
            &self.ledger_path,
            self.sigverify_disabled,
            entry_height,
        );

        let broadcast_stage = BroadcastStage::new(
            self.broadcast_socket
                .try_clone()
                .expect("Failed to clone broadcast socket"),
            self.blockthread.clone(),
            self.shared_window.clone(),
            entry_height,
            blob_receiver,
            tx_creator_exit,
        );
        let leader_state = LeaderServices::new(tx_creator, broadcast_stage);
        self.node_role = Some(NodeRole::Leader(leader_state));
    }

    pub fn handle_role_transition(&mut self) -> Result<Option<FullnodeReturnType>> {
        let node_role = self.node_role.take();
        match node_role {
            Some(NodeRole::Leader(leader_services)) => match leader_services.join()? {
                Some(TxCreatorReturnType::LeaderRotation) => {
                    self.leader_to_validator()?;
                    Ok(Some(FullnodeReturnType::LeaderRotation))
                }
                _ => Ok(None),
            },
            Some(NodeRole::Validator(validator_services)) => match validator_services.join()? {
                Some(TxSignerReturnType::LeaderRotation(entry_height)) => {
                    self.validator_to_leader(entry_height);
                    Ok(Some(FullnodeReturnType::LeaderRotation))
                }
                _ => Ok(None),
            },
            None => Ok(None),
        }
    }

    //used for notifying many nodes in parallel to exit
    pub fn exit(&self) {
        self.exit.store(true, Ordering::Relaxed);
        if let Some(ref rpu) = self.rpu {
            rpu.exit();
        }
        match self.node_role {
            Some(NodeRole::Leader(ref leader_services)) => leader_services.exit(),
            Some(NodeRole::Validator(ref validator_services)) => validator_services.exit(),
            _ => (),
        }
    }

    pub fn close(self) -> Result<(Option<FullnodeReturnType>)> {
        self.exit();
        self.join()
    }

    // TODO: only used for testing, get rid of this once we have actual
    // leader scheduling
    pub fn set_scheduled_leader(&self, leader_id: Pubkey, entry_height: u64) {
        self.blockthread
            .write()
            .unwrap()
            .set_scheduled_leader(entry_height, leader_id);
    }

    fn new_transaction_processor_from_ledger(ledger_path: &str) -> (TransactionProcessor, u64, Vec<Entry>) {
        let transaction_processor = TransactionProcessor::new_default(false);
        let entries = read_ledger(ledger_path, true).expect("opening ledger");
        let entries = entries
            .map(|e| e.unwrap_or_else(|err| panic!("failed to parse entry. error: {}", err)));
        info!("processing ledger...");
        let (entry_height, ledger_tail) = transaction_processor.process_ledger(entries).expect("process_ledger");
        // entry_height is the network-wide agreed height of the ledger.
        //  initialize it from the input ledger
        info!("processed {} ledger...", entry_height);
        (transaction_processor, entry_height, ledger_tail)
    }
}

impl Service for Fullnode {
    type JoinReturnType = Option<FullnodeReturnType>;

    fn join(self) -> Result<Option<FullnodeReturnType>> {
        if let Some(rpu) = self.rpu {
            rpu.join()?;
        }
        self.ncp.join()?;
        self.rpc_service.join()?;

        match self.node_role {
            Some(NodeRole::Validator(validator_service)) => {
                if let Some(TxSignerReturnType::LeaderRotation(_)) = validator_service.join()? {
                    return Ok(Some(FullnodeReturnType::LeaderRotation));
                }
            }
            Some(NodeRole::Leader(leader_service)) => {
                if let Some(TxCreatorReturnType::LeaderRotation) = leader_service.join()? {
                    return Ok(Some(FullnodeReturnType::LeaderRotation));
                }
            }
            _ => (),
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use transaction_processor::TransactionProcessor;
    use blockthread::Node;
    use fullnode::{Fullnode, FullnodeReturnType};
    use ledger::genesis;
    use packet::make_consecutive_blobs;
    use service::Service;
    use signature::{Keypair, KeypairUtil};
    use std::cmp;
    use std::fs::remove_dir_all;
    use std::net::UdpSocket;
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use streamer::responder;

    #[test]
    fn validator_exit() {
        let keypair = Keypair::new();
        let tn = Node::new_localhost_with_pubkey(keypair.pubkey());
        let (alice, validator_ledger_path) = genesis("validator_exit", 10_000);
        let transaction_processor = TransactionProcessor::new(&alice);
        let entry = tn.info.clone();
        let v = Fullnode::new_with_transaction_processor(
            keypair,
            transaction_processor,
            0,
            &[],
            tn,
            Some(&entry),
            &validator_ledger_path,
            false,
            None,
            Some(0),
        );
        v.close().unwrap();
        remove_dir_all(validator_ledger_path).unwrap();
    }

    #[test]
    fn validator_parallel_exit() {
        let mut ledger_paths = vec![];
        let vals: Vec<Fullnode> = (0..2)
            .map(|i| {
                let keypair = Keypair::new();
                let tn = Node::new_localhost_with_pubkey(keypair.pubkey());
                let (alice, validator_ledger_path) =
                    genesis(&format!("validator_parallel_exit_{}", i), 10_000);
                ledger_paths.push(validator_ledger_path.clone());
                let transaction_processor = TransactionProcessor::new(&alice);
                let entry = tn.info.clone();
                Fullnode::new_with_transaction_processor(
                    keypair,
                    transaction_processor,
                    0,
                    &[],
                    tn,
                    Some(&entry),
                    &validator_ledger_path,
                    false,
                    None,
                    Some(0),
                )
            }).collect();

        //each validator can exit in parallel to speed many sequential calls to `join`
        vals.iter().for_each(|v| v.exit());
        //while join is called sequentially, the above exit call notified all the
        //validators to exit from all their threads
        vals.into_iter().for_each(|v| {
            v.join().unwrap();
        });

        for path in ledger_paths {
            remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn test_validator_to_leader_transition() {
        // Make a leader identity
        let leader_keypair = Keypair::new();
        let leader_node = Node::new_localhost_with_pubkey(leader_keypair.pubkey());
        let leader_id = leader_node.info.id;
        let leader_ncp = leader_node.info.contact_info.ncp;

        // Start the validator node
        let leader_rotation_interval = 10;
        let (mint, validator_ledger_path) = genesis("test_validator_to_leader_transition", 10_000);
        let validator_keypair = Keypair::new();
        let validator_node = Node::new_localhost_with_pubkey(validator_keypair.pubkey());
        let validator_info = validator_node.info.clone();
        let mut validator = Fullnode::new(
            validator_node,
            &validator_ledger_path,
            validator_keypair,
            Some(leader_ncp),
            false,
            Some(leader_rotation_interval),
        );

        // Set the leader schedule for the validator
        let my_leader_begin_epoch = 2;
        for i in 0..my_leader_begin_epoch {
            validator.set_scheduled_leader(leader_id, leader_rotation_interval * i);
        }
        validator.set_scheduled_leader(
            validator_info.id,
            my_leader_begin_epoch * leader_rotation_interval,
        );

        // Send blobs to the validator from our mock leader
        let t_responder = {
            let (s_responder, r_responder) = channel();
            let blob_sockets: Vec<Arc<UdpSocket>> = leader_node
                .sockets
                .replicate
                .into_iter()
                .map(Arc::new)
                .collect();

            let t_responder = responder(
                "test_validator_to_leader_transition",
                blob_sockets[0].clone(),
                r_responder,
            );

            // Send the blobs out of order, in reverse. Also send an extra
            // "extra_blobs" number of blobs to make sure the window stops in the right place.
            let extra_blobs = cmp::max(leader_rotation_interval / 3, 1);
            let total_blobs_to_send =
                my_leader_begin_epoch * leader_rotation_interval + extra_blobs;
            let genesis_entries = mint.create_entries();
            let last_id = genesis_entries
                .last()
                .expect("expected at least one genesis entry")
                .id;
            let tx_signer_address = &validator_info.contact_info.tx_signer;
            let msgs =
                make_consecutive_blobs(leader_id, total_blobs_to_send, last_id, &tx_signer_address)
                    .into_iter()
                    .rev()
                    .collect();
            s_responder.send(msgs).expect("send");
            t_responder
        };

        // Wait for validator to shut down tx_signer and restart tx_creator
        match validator.handle_role_transition().unwrap() {
            Some(FullnodeReturnType::LeaderRotation) => (),
            _ => panic!("Expected reason for exit to be leader rotation"),
        }

        // Check the validator ledger to make sure it's the right height
        let (_, entry_height, _) = Fullnode::new_transaction_processor_from_ledger(&validator_ledger_path);

        assert_eq!(
            entry_height,
            my_leader_begin_epoch * leader_rotation_interval
        );

        // Shut down
        t_responder.join().expect("responder thread join");
        validator.close().unwrap();
        remove_dir_all(&validator_ledger_path).unwrap();
    }
}
