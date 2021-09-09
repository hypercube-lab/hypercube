use bincode::serialize;
use hash::{Hash, Hasher};
use signature::{Keypair, KeypairUtil, Signature};
use xpz_program_interface::pubkey::Pubkey;
use std::mem::size_of;

pub const SIGNED_DATA_OFFSET: usize = size_of::<Signature>();
pub const SIG_OFFSET: usize = 0;
pub const PUB_KEY_OFFSET: usize = size_of::<Signature>() + size_of::<u64>();


#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Transaction {
    pub signature: Signature,
    pub keys: Vec<Pubkey>,
    pub program_id: Pubkey,
    pub last_id: Hash,
    pub fee: i64,
    pub userdata: Vec<u8>,
}

impl Transaction {
    pub fn new(
        from_keypair: &Keypair,
        transaction_keys: &[Pubkey],
        program_id: Pubkey,
        userdata: Vec<u8>,
        last_id: Hash,
        fee: i64,
    ) -> Self {
        let from = from_keypair.pubkey();
        let mut keys = vec![from];
        keys.extend_from_slice(transaction_keys);
        let mut tx = Transaction {
            signature: Signature::default(),
            keys,
            program_id,
            last_id,
            fee,
            userdata,
        };
        tx.sign(from_keypair);
        tx
    }


    pub fn get_sign_data(&self) -> Vec<u8> {
        let mut data = serialize(&(&self.keys)).expect("serialize keys");

        let program_id = serialize(&(&self.program_id)).expect("serialize program_id");
        data.extend_from_slice(&program_id);

        let last_id_data = serialize(&(&self.last_id)).expect("serialize last_id");
        data.extend_from_slice(&last_id_data);

        let fee_data = serialize(&(&self.fee)).expect("serialize last_id");
        data.extend_from_slice(&fee_data);

        let userdata = serialize(&(&self.userdata)).expect("serialize userdata");
        data.extend_from_slice(&userdata);
        data
    }

    pub fn sign(&mut self, keypair: &Keypair) {
        let sign_data = self.get_sign_data();
        self.signature = Signature::new(keypair.sign(&sign_data).as_ref());
    }


    pub fn verify_signature(&self) -> bool {
        warn!("transaction signature verification called");
        self.signature
            .verify(&self.from().as_ref(), &self.get_sign_data())
    }

    pub fn from(&self) -> &Pubkey {
        &self.keys[0]
    }


    pub fn hash(transactions: &[Transaction]) -> Hash {
        let mut hasher = Hasher::default();
        transactions
            .iter()
            .for_each(|tx| hasher.hash(&tx.signature.as_ref()));
        hasher.result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::serialize;
    use signature::GenKeys;
    #[test]
    fn test_sdk_serialize() {
        let keypair = &GenKeys::new([0u8; 32]).gen_n_keypairs(1)[0];
        let to = Pubkey::new(&[
            1, 1, 1, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 8, 7, 6, 5, 4,
            1, 1, 1,
        ]);

        let program_id = Pubkey::new(&[
            2, 2, 2, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 8, 7, 6, 5, 4,
            2, 2, 2,
        ]);

        let tx = Transaction::new(
            keypair,
            &[keypair.pubkey(), to],
            program_id,
            vec![1, 2, 3],
            Hash::default(),
            99,
        );
        assert_eq!(
            serialize(&tx).unwrap(),
            vec![
                88, 1, 212, 176, 31, 197, 35, 156, 135, 24, 30, 57, 204, 253, 224, 28, 89, 189, 53,
                64, 27, 148, 42, 199, 43, 236, 85, 182, 150, 64, 96, 53, 255, 235, 90, 197, 228, 6,
                105, 22, 140, 209, 206, 221, 85, 117, 125, 126, 11, 1, 176, 130, 57, 236, 7, 155,
                127, 58, 130, 92, 230, 219, 254, 0, 3, 0, 0, 0, 0, 0, 0, 0, 32, 253, 186, 201, 177,
                11, 117, 135, 187, 167, 181, 188, 22, 59, 206, 105, 231, 150, 215, 30, 78, 212, 76,
                16, 252, 180, 72, 134, 137, 247, 161, 68, 32, 253, 186, 201, 177, 11, 117, 135,
                187, 167, 181, 188, 22, 59, 206, 105, 231, 150, 215, 30, 78, 212, 76, 16, 252, 180,
                72, 134, 137, 247, 161, 68, 1, 1, 1, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
                9, 9, 9, 9, 9, 9, 8, 7, 6, 5, 4, 1, 1, 1, 2, 2, 2, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 8, 7, 6, 5, 4, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 99, 0, 0, 0, 0,
                0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3
            ],
        );
    }
}
