use blockthread::{BlockThreadError, NodeInfo};
use rand::distributions::{Distribution, Weighted, WeightedChoice};
use rand::thread_rng;
use result::Result;
use xpz_program_interface::pubkey::Pubkey;
use std;
use std::collections::HashMap;

pub const DEFAULT_WEIGHT: u32 = 1;

pub trait ChooseGossipPeerStrategy {
    fn choose_peer<'a>(&self, options: Vec<&'a NodeInfo>) -> Result<&'a NodeInfo>;
}

pub struct ChooseRandomPeerStrategy<'a> {
    random: &'a Fn() -> u64,
}

// Given a source of randomness "random", this strategy will randomly pick a validator
// from the input options. This strategy works in isolation, but doesn't leverage any
// rumors from the rest of the gossip network to make more informed decisions about
// which validators have more/less updates
impl<'a, 'b> ChooseRandomPeerStrategy<'a> {
    pub fn new(random: &'a Fn() -> u64) -> Self {
        ChooseRandomPeerStrategy { random }
    }
}

impl<'a> ChooseGossipPeerStrategy for ChooseRandomPeerStrategy<'a> {
    fn choose_peer<'b>(&self, options: Vec<&'b NodeInfo>) -> Result<&'b NodeInfo> {
        if options.is_empty() {
            Err(BlockThreadError::NoPeers)?;
        }

        let n = ((self.random)() as usize) % options.len();
        Ok(options[n])
    }
}



pub struct ChooseWeightedPeerStrategy<'a> {
    
    remote: &'a HashMap<Pubkey, u64>,
    
    external_liveness: &'a HashMap<Pubkey, HashMap<Pubkey, u64>>,
    
    get_stake: &'a Fn(Pubkey) -> f64,
}

impl<'a> ChooseWeightedPeerStrategy<'a> {
    pub fn new(
        remote: &'a HashMap<Pubkey, u64>,
        external_liveness: &'a HashMap<Pubkey, HashMap<Pubkey, u64>>,
        get_stake: &'a Fn(Pubkey) -> f64,
    ) -> Self {
        ChooseWeightedPeerStrategy {
            remote,
            external_liveness,
            get_stake,
        }
    }

    fn calculate_weighted_remote_index(&self, peer_id: Pubkey) -> u32 {
        let mut last_seen_index = 0;
        
        if let Some(index) = self.remote.get(&peer_id) {
            last_seen_index = *index;
        }

        let liveness_entry = self.external_liveness.get(&peer_id);
        if liveness_entry.is_none() {
            return DEFAULT_WEIGHT;
        }

        let votes = liveness_entry.unwrap();

        if votes.is_empty() {
            return DEFAULT_WEIGHT;
        }

        
        let mut relevant_votes = vec![];

        let total_stake = votes.iter().fold(0.0, |total_stake, (&id, &vote)| {
            let stake = (self.get_stake)(id);
            
            if std::f64::MAX - total_stake < stake {
                if stake > total_stake {
                    relevant_votes = vec![(stake, vote)];
                    stake
                } else {
                    total_stake
                }
            } else {
                relevant_votes.push((stake, vote));
                total_stake + stake
            }
        });

        let weighted_vote = relevant_votes.iter().fold(0.0, |sum, &(stake, vote)| {
            if vote < last_seen_index {
                

                warn!("weighted peer index was smaller than local entry in remote table");
                return sum;
            }

            let vote_difference = (vote - last_seen_index) as f64;
            let new_weight = vote_difference * (stake / total_stake);

            if std::f64::MAX - sum < new_weight {
                return f64::max(new_weight, sum);
            }

            sum + new_weight
        });

        
        if weighted_vote >= f64::from(std::u32::MAX) {
            return std::u32::MAX;
        }

        
        weighted_vote as u32 + DEFAULT_WEIGHT
    }
}

impl<'a> ChooseGossipPeerStrategy for ChooseWeightedPeerStrategy<'a> {
    fn choose_peer<'b>(&self, options: Vec<&'b NodeInfo>) -> Result<&'b NodeInfo> {
        if options.is_empty() {
            Err(BlockThreadError::NoPeers)?;
        }

        let mut weighted_peers = vec![];
        for peer in options {
            let weight = self.calculate_weighted_remote_index(peer.id);
            weighted_peers.push(Weighted { weight, item: peer });
        }

        let mut rng = thread_rng();
        Ok(WeightedChoice::new(&mut weighted_peers).sample(&mut rng))
    }
}

#[cfg(test)]
mod tests {
    use choose_gossip_peer_strategy::{ChooseWeightedPeerStrategy, DEFAULT_WEIGHT};
    use logger;
    use signature::{Keypair, KeypairUtil};
    use xpz_program_interface::pubkey::Pubkey;
    use std;
    use std::collections::HashMap;

    fn get_stake(_id: Pubkey) -> f64 {
        1.0
    }

    #[test]
    fn test_default() {
        logger::setup();

        
        let key1 = Keypair::new().pubkey();

        let remote: HashMap<Pubkey, u64> = HashMap::new();
        let external_liveness: HashMap<Pubkey, HashMap<Pubkey, u64>> = HashMap::new();

        let weighted_strategy =
            ChooseWeightedPeerStrategy::new(&remote, &external_liveness, &get_stake);

        
        let result = weighted_strategy.calculate_weighted_remote_index(key1);
        assert_eq!(result, DEFAULT_WEIGHT);
    }

    #[test]
    fn test_only_external_liveness() {
        logger::setup();

        
        let key1 = Keypair::new().pubkey();
        let key2 = Keypair::new().pubkey();

        let remote: HashMap<Pubkey, u64> = HashMap::new();
        let mut external_liveness: HashMap<Pubkey, HashMap<Pubkey, u64>> = HashMap::new();

        
        let test_value: u32 = 5;
        let mut rumors: HashMap<Pubkey, u64> = HashMap::new();
        rumors.insert(key2, test_value as u64);
        external_liveness.insert(key1, rumors);

        let weighted_strategy =
            ChooseWeightedPeerStrategy::new(&remote, &external_liveness, &get_stake);

        let result = weighted_strategy.calculate_weighted_remote_index(key1);
        assert_eq!(result, test_value + DEFAULT_WEIGHT);
    }

    #[test]
    fn test_overflow_votes() {
        logger::setup();

        
        let key1 = Keypair::new().pubkey();
        let key2 = Keypair::new().pubkey();

        let remote: HashMap<Pubkey, u64> = HashMap::new();
        let mut external_liveness: HashMap<Pubkey, HashMap<Pubkey, u64>> = HashMap::new();

        
        let test_value = (std::u32::MAX as u64) + 10;
        let mut rumors: HashMap<Pubkey, u64> = HashMap::new();
        rumors.insert(key2, test_value);
        external_liveness.insert(key1, rumors);

        let weighted_strategy =
            ChooseWeightedPeerStrategy::new(&remote, &external_liveness, &get_stake);

        let result = weighted_strategy.calculate_weighted_remote_index(key1);
        assert_eq!(result, std::u32::MAX);
    }

    #[test]
    fn test_many_validators() {
        logger::setup();

        
        let key1 = Keypair::new().pubkey();

        let mut remote: HashMap<Pubkey, u64> = HashMap::new();
        let mut external_liveness: HashMap<Pubkey, HashMap<Pubkey, u64>> = HashMap::new();

        let num_peers = 10;
        let mut rumors: HashMap<Pubkey, u64> = HashMap::new();

        remote.insert(key1, 0);

        for i in 0..num_peers {
            let pubkey = Keypair::new().pubkey();
            rumors.insert(pubkey, i);
        }

        external_liveness.insert(key1, rumors);

        let weighted_strategy =
            ChooseWeightedPeerStrategy::new(&remote, &external_liveness, &get_stake);

        let result = weighted_strategy.calculate_weighted_remote_index(key1);
        assert_eq!(result, (num_peers / 2) as u32);
    }

    #[test]
    fn test_many_validators2() {
        logger::setup();

        let key1 = Keypair::new().pubkey();

        let mut remote: HashMap<Pubkey, u64> = HashMap::new();
        let mut external_liveness: HashMap<Pubkey, HashMap<Pubkey, u64>> = HashMap::new();

        let num_peers = 10;
        let old_index = 20;
        let mut rumors: HashMap<Pubkey, u64> = HashMap::new();

        remote.insert(key1, old_index);

        for _i in 0..num_peers {
            let pubkey = Keypair::new().pubkey();
            rumors.insert(pubkey, old_index);
        }

        external_liveness.insert(key1, rumors);

        let weighted_strategy =
            ChooseWeightedPeerStrategy::new(&remote, &external_liveness, &get_stake);

        let result = weighted_strategy.calculate_weighted_remote_index(key1);

        assert_eq!(result, DEFAULT_WEIGHT);
    }
}
