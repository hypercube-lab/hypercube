//! The `vote_stage` votes on the `last_id` of the transaction_processor at a regular cadence

use transaction_processor::TransactionProcessor;
use bincode::serialize;
use fin_plan_transaction::BudgetTransaction;
use counter::Counter;
use blockthread::BlockThread;
use hash::Hash;
use influx_db_client as influxdb;
use log::Level;
use metrics;
use packet::SharedBlob;
use result::Result;
use signature::Keypair;
use xpz_program_interface::pubkey::Pubkey;
use std::result;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, RwLock};
use streamer::BlobSender;
use timing;
use transaction::Transaction;

pub const VOTE_TIMEOUT_MS: u64 = 1000;

#[derive(Debug, PartialEq, Eq)]
enum VoteError {
    NoValidLastIdsToVoteOn,
}

pub fn create_new_signed_vote_blob(
    last_id: &Hash,
    keypair: &Keypair,
    blockthread: &Arc<RwLock<BlockThread>>,
) -> Result<SharedBlob> {
    let shared_blob = SharedBlob::default();
    let (vote, addr) = {
        let mut wblockthread = blockthread.write().unwrap();
        //TODO: doesn't seem like there is a synchronous call to get height and id
        debug!("voting on {:?}", &last_id.as_ref()[..8]);
        wblockthread.new_vote(*last_id)
    }?;
    let tx = Transaction::fin_plan_new_vote(&keypair, vote, *last_id, 0);
    {
        let mut blob = shared_blob.write().unwrap();
        let bytes = serialize(&tx)?;
        let len = bytes.len();
        blob.data[..len].copy_from_slice(&bytes);
        blob.meta.set_addr(&addr);
        blob.meta.size = len;
    }
    Ok(shared_blob)
}

fn get_last_id_to_vote_on(
    id: &Pubkey,
    ids: &[Hash],
    transaction_processor: &Arc<TransactionProcessor>,
    now: u64,
    last_vote: &mut u64,
    last_valid_validator_timestamp: &mut u64,
) -> result::Result<(Hash, u64), VoteError> {
    let mut valid_ids = transaction_processor.count_valid_ids(&ids);
    let super_majority_index = (2 * ids.len()) / 3;

    //TODO(anatoly): this isn't stake based voting
    debug!(
        "{}: valid_ids {}/{} {}",
        id,
        valid_ids.len(),
        ids.len(),
        super_majority_index,
    );

    metrics::submit(
        influxdb::Point::new("vote_stage-peer_count")
            .add_field("total_peers", influxdb::Value::Integer(ids.len() as i64))
            .add_field(
                "valid_peers",
                influxdb::Value::Integer(valid_ids.len() as i64),
            ).to_owned(),
    );

    if valid_ids.len() > super_majority_index {
        *last_vote = now;

        // Sort by timestamp
        valid_ids.sort_by(|a, b| a.1.cmp(&b.1));

        let last_id = ids[valid_ids[super_majority_index].0];
        return Ok((last_id, valid_ids[super_majority_index].1));
    }

    if *last_valid_validator_timestamp != 0 {
        metrics::submit(
            influxdb::Point::new(&"leader-finality")
                .add_field(
                    "duration_ms",
                    influxdb::Value::Integer((now - *last_valid_validator_timestamp) as i64),
                ).to_owned(),
        );
    }

    Err(VoteError::NoValidLastIdsToVoteOn)
}

pub fn send_leader_vote(
    id: &Pubkey,
    keypair: &Keypair,
    transaction_processor: &Arc<TransactionProcessor>,
    blockthread: &Arc<RwLock<BlockThread>>,
    vote_blob_sender: &BlobSender,
    last_vote: &mut u64,
    last_valid_validator_timestamp: &mut u64,
) -> Result<()> {
    let now = timing::timestamp();
    if now - *last_vote > VOTE_TIMEOUT_MS {
        let ids: Vec<_> = blockthread.read().unwrap().valid_last_ids();
        if let Ok((last_id, super_majority_timestamp)) = get_last_id_to_vote_on(
            id,
            &ids,
            transaction_processor,
            now,
            last_vote,
            last_valid_validator_timestamp,
        ) {
            if let Ok(shared_blob) = create_new_signed_vote_blob(&last_id, keypair, blockthread) {
                vote_blob_sender.send(vec![shared_blob])?;
                let finality_ms = now - super_majority_timestamp;

                *last_valid_validator_timestamp = super_majority_timestamp;
                debug!("{} leader_sent_vote finality: {} ms", id, finality_ms);
                inc_new_counter_info!("vote_stage-leader_sent_vote", 1);

                transaction_processor.set_finality((now - *last_valid_validator_timestamp) as usize);

                metrics::submit(
                    influxdb::Point::new(&"leader-finality")
                        .add_field("duration_ms", influxdb::Value::Integer(finality_ms as i64))
                        .to_owned(),
                );
            }
        }
    }
    Ok(())
}

pub fn send_validator_vote(
    transaction_processor: &Arc<TransactionProcessor>,
    keypair: &Arc<Keypair>,
    blockthread: &Arc<RwLock<BlockThread>>,
    vote_blob_sender: &BlobSender,
) -> Result<()> {
    let last_id = transaction_processor.last_id();
    if let Ok(shared_blob) = create_new_signed_vote_blob(&last_id, keypair, blockthread) {
        inc_new_counter_info!("replicate-vote_sent", 1);

        vote_blob_sender.send(vec![shared_blob])?;
    }
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use transaction_processor::TransactionProcessor;
    use bincode::deserialize;
    use fin_plan_instruction::Vote;
    use blockthread::{BlockThread, NodeInfo};
    use entry::next_entry;
    use hash::{hash, Hash};
    use logger;
    use mint::Mint;
    use std::sync::mpsc::channel;
    use std::sync::{Arc, RwLock};
    use std::thread::sleep;
    use std::time::Duration;
    use builtin_tansaction::SystemTransaction;
    use transaction::Transaction;

    #[test]
    fn test_send_leader_vote() {
        logger::setup();

        // create a mint/transaction_processor
        let mint = Mint::new(1000);
        let transaction_processor = Arc::new(TransactionProcessor::new(&mint));
        let hash0 = Hash::default();

        // get a non-default hash last_id
        let entry = next_entry(&hash0, 1, vec![]);
        transaction_processor.register_entry_id(&entry.id);

        // Create a leader
        let leader_data = NodeInfo::new_with_socketaddr(&"127.0.0.1:1234".parse().unwrap());
        let leader_pubkey = leader_data.id.clone();
        let mut leader_blockthread = BlockThread::new(leader_data).unwrap();

        // give the leader some tokens
        let give_leader_tokens_tx =
            Transaction::system_new(&mint.keypair(), leader_pubkey.clone(), 100, entry.id);
        transaction_processor.process_transaction(&give_leader_tokens_tx).unwrap();

        leader_blockthread.set_leader(leader_pubkey);

        // Insert 7 agreeing validators / 3 disagreeing
        // and votes for new last_id
        for i in 0..10 {
            let mut validator =
                NodeInfo::new_with_socketaddr(&format!("127.0.0.1:234{}", i).parse().unwrap());

            let vote = Vote {
                version: validator.version + 1,
                contact_info_version: 1,
            };

            if i < 7 {
                validator.ledger_state.last_id = entry.id;
            }

            leader_blockthread.insert(&validator);
            trace!("validator id: {:?}", validator.id);

            leader_blockthread.insert_vote(&validator.id, &vote, entry.id);
        }
        let leader = Arc::new(RwLock::new(leader_blockthread));
        let (vote_blob_sender, vote_blob_receiver) = channel();
        let mut last_vote: u64 = timing::timestamp() - VOTE_TIMEOUT_MS - 1;
        let mut last_valid_validator_timestamp = 0;
        let res = send_leader_vote(
            &mint.pubkey(),
            &mint.keypair(),
            &transaction_processor,
            &leader,
            &vote_blob_sender,
            &mut last_vote,
            &mut last_valid_validator_timestamp,
        );
        trace!("vote result: {:?}", res);
        assert!(res.is_ok());
        let vote_blob = vote_blob_receiver.recv_timeout(Duration::from_millis(500));
        trace!("vote_blob: {:?}", vote_blob);

        // leader shouldn't vote yet, not enough votes
        assert!(vote_blob.is_err());

        // add two more nodes and see that it succeeds
        for i in 0..2 {
            let mut validator =
                NodeInfo::new_with_socketaddr(&format!("127.0.0.1:234{}", i).parse().unwrap());

            let vote = Vote {
                version: validator.version + 1,
                contact_info_version: 1,
            };

            validator.ledger_state.last_id = entry.id;

            leader.write().unwrap().insert(&validator);
            trace!("validator id: {:?}", validator.id);

            leader
                .write()
                .unwrap()
                .insert_vote(&validator.id, &vote, entry.id);
        }

        last_vote = timing::timestamp() - VOTE_TIMEOUT_MS - 1;
        let res = send_leader_vote(
            &Pubkey::default(),
            &mint.keypair(),
            &transaction_processor,
            &leader,
            &vote_blob_sender,
            &mut last_vote,
            &mut last_valid_validator_timestamp,
        );
        trace!("vote result: {:?}", res);
        assert!(res.is_ok());
        let vote_blob = vote_blob_receiver.recv_timeout(Duration::from_millis(500));
        trace!("vote_blob: {:?}", vote_blob);

        // leader should vote now
        assert!(vote_blob.is_ok());

        // vote should be valid
        let blob = &vote_blob.unwrap()[0];
        let tx = deserialize(&(blob.read().unwrap().data)).unwrap();
        assert!(transaction_processor.process_transaction(&tx).is_ok());
    }

    #[test]
    fn test_get_last_id_to_vote_on() {
        logger::setup();

        let mint = Mint::new(1234);
        let transaction_processor = Arc::new(TransactionProcessor::new(&mint));
        let mut last_vote = 0;
        let mut last_valid_validator_timestamp = 0;

        // generate 10 last_ids, register 6 with the transaction_processor
        let ids: Vec<_> = (0..10)
            .map(|i| {
                let last_id = hash(&serialize(&i).unwrap()); // Unique hash
                if i < 6 {
                    transaction_processor.register_entry_id(&last_id);
                }
                // sleep to get a different timestamp in the transaction_processor
                sleep(Duration::from_millis(1));
                last_id
            }).collect();

        // see that we fail to have 2/3rds consensus
        assert!(
            get_last_id_to_vote_on(
                &Pubkey::default(),
                &ids,
                &transaction_processor,
                0,
                &mut last_vote,
                &mut last_valid_validator_timestamp
            ).is_err()
        );

        // register another, see passing
        transaction_processor.register_entry_id(&ids[6]);

        let res = get_last_id_to_vote_on(
            &Pubkey::default(),
            &ids,
            &transaction_processor,
            0,
            &mut last_vote,
            &mut last_valid_validator_timestamp,
        );
        if let Ok((hash, timestamp)) = res {
            assert!(hash == ids[6]);
            assert!(timestamp != 0);
        } else {
            assert!(false, "get_last_id returned error!: {:?}", res);
        }
    }
}
