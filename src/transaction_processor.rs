use bincode::deserialize;
use bincode::serialize;
use fin_plan_program::FinPlanState;
use fin_plan_transaction::FinPlanTransaction;
use counter::Counter;
use dynamic_program::DynamicProgram;
use entry::Entry;
use hash::{hash, Hash};
use itertools::Itertools;
use ledger::Block;
use log::Level;
use mint::Mint;
use trx_out::Payment;
use signature::{Keypair, Signature};
use xpz_program_interface::account::{Account, KeyedAccount};
use xpz_program_interface::pubkey::Pubkey;
use std;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Instant;
use storage_program::StorageProgram;
use builtin_pgm::SystemProgram;
use builtin_tansaction::SystemTransaction;
use tictactoe_dashboard_program::TicTacToeDashboardProgram;
use tictactoe_program::TicTacToeProgram;
use timing::{duration_as_us, timestamp};
use transaction::Transaction;
use window::WINDOW_SIZE;

pub const MAX_ENTRY_IDS: usize = 1024 * 16;

pub const VERIFY_BLOCK_SIZE: usize = 16;


#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TransactionProcessorError {
    
    AccountNotFound,
  
    InsufficientFundsForFee,
    
    DuplicateSignature,

    LastIdNotFound,

    SignatureNotFound,

    LedgerVerificationFailed,

    UnbalancedTransaction,

    ResultWithNegativeTokens,

    UnknownContractId,

    ModifiedContractId,

    ExternalAccountTokenSpend,

    ProgramRuntimeError,
}

pub type Result<T> = result::Result<T, TransactionProcessorError>;
type SignatureStatusMap = HashMap<Signature, Result<()>>;

#[derive(Default)]
struct ErrorCounters {
    account_not_found_validator: usize,
    account_not_found_leader: usize,
    account_not_found_vote: usize,
}

pub struct TransactionProcessor {
    accounts: RwLock<HashMap<Pubkey, Account>>,

    last_ids: RwLock<VecDeque<Hash>>,

    last_ids_sigs: RwLock<HashMap<Hash, (SignatureStatusMap, u64)>>,

    transaction_count: AtomicUsize,

    pub is_leader: bool,

    finality_time: AtomicUsize,

    loaded_contracts: RwLock<HashMap<Pubkey, DynamicProgram>>,
}

impl Default for TransactionProcessor {
    fn default() -> Self {
        TransactionProcessor {
            accounts: RwLock::new(HashMap::new()),
            last_ids: RwLock::new(VecDeque::new()),
            last_ids_sigs: RwLock::new(HashMap::new()),
            transaction_count: AtomicUsize::new(0),
            is_leader: true,
            finality_time: AtomicUsize::new(std::usize::MAX),
            loaded_contracts: RwLock::new(HashMap::new()),
        }
    }
}

impl TransactionProcessor {

    pub fn new_default(is_leader: bool) -> Self {
        let mut transaction_processor = TransactionProcessor::default();
        transaction_processor.is_leader = is_leader;
        transaction_processor
    }

    pub fn new_from_deposit(deposit: &Payment) -> Self {
        let transaction_processor = Self::default();
        {
            let mut accounts = transaction_processor.accounts.write().unwrap();
            let account = accounts.entry(deposit.to).or_insert_with(Account::default);
            Self::apply_payment(deposit, account);
        }
        transaction_processor
    }

    pub fn new(mint: &Mint) -> Self {
        let deposit = Payment {
            to: mint.pubkey(),
            tokens: mint.tokens,
        };
        let transaction_processor = Self::new_from_deposit(&deposit);
        transaction_processor.register_entry_id(&mint.last_id());
        transaction_processor
    }

    fn apply_payment(payment: &Payment, account: &mut Account) {
        trace!("apply payments {}", payment.tokens);
        account.tokens += payment.tokens;
    }


    pub fn last_id(&self) -> Hash {
        let last_ids = self.last_ids.read().expect("'last_ids' read lock");
        let last_item = last_ids
            .iter()
            .last()
            .expect("get last item from 'last_ids' list");
        *last_item
    }

    fn reserve_signature(signatures: &mut SignatureStatusMap, signature: &Signature) -> Result<()> {
        if let Some(_result) = signatures.get(signature) {
            return Err(TransactionProcessorError::DuplicateSignature);
        }
        signatures.insert(*signature, Ok(()));
        Ok(())
    }

    pub fn clear_signatures(&self) {
        for (_, sigs) in self.last_ids_sigs.write().unwrap().iter_mut() {
            sigs.0.clear();
        }
    }

    fn reserve_signature_with_last_id(&self, signature: &Signature, last_id: &Hash) -> Result<()> {
        if let Some(entry) = self
            .last_ids_sigs
            .write()
            .expect("'last_ids' read lock in reserve_signature_with_last_id")
            .get_mut(last_id)
        {
            return Self::reserve_signature(&mut entry.0, signature);
        }
        Err(TransactionProcessorError::LastIdNotFound)
    }

    fn update_signature_status(
        signatures: &mut SignatureStatusMap,
        signature: &Signature,
        result: &Result<()>,
    ) {
        let entry = signatures.entry(*signature).or_insert(Ok(()));
        *entry = result.clone();
    }

    fn update_signature_status_with_last_id(
        &self,
        signature: &Signature,
        result: &Result<()>,
        last_id: &Hash,
    ) {
        if let Some(entry) = self.last_ids_sigs.write().unwrap().get_mut(last_id) {
            Self::update_signature_status(&mut entry.0, signature, result);
        }
    }

    fn update_transaction_statuses(&self, txs: &[Transaction], res: &[Result<()>]) {
        for (i, tx) in txs.iter().enumerate() {
            self.update_signature_status_with_last_id(&tx.signature, &res[i], &tx.last_id);
        }
    }


    pub fn count_valid_ids(&self, ids: &[Hash]) -> Vec<(usize, u64)> {
        let last_ids = self.last_ids_sigs.read().unwrap();
        let mut ret = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            if let Some(entry) = last_ids.get(id) {
                ret.push((i, entry.1));
            }
        }
        ret
    }


    pub fn register_entry_id(&self, last_id: &Hash) {
        let mut last_ids = self
            .last_ids
            .write()
            .expect("'last_ids' write lock in register_entry_id");
        let mut last_ids_sigs = self
            .last_ids_sigs
            .write()
            .expect("last_ids_sigs write lock");
        if last_ids.len() >= MAX_ENTRY_IDS {
            let id = last_ids.pop_front().unwrap();
            last_ids_sigs.remove(&id);
        }
        last_ids_sigs.insert(*last_id, (HashMap::new(), timestamp()));
        last_ids.push_back(*last_id);
    }

    pub fn process_transaction(&self, tx: &Transaction) -> Result<()> {
        match self.process_transactions(&[tx.clone()])[0] {
            Err(ref e) => {
                info!("process_transaction error: {:?}", e);
                Err((*e).clone())
            }
            Ok(_) => Ok(()),
        }
    }

    fn load_account(
        &self,
        tx: &Transaction,
        accounts: &HashMap<Pubkey, Account>,
        error_counters: &mut ErrorCounters,
    ) -> Result<Vec<Account>> {

        if accounts.get(&tx.keys[0]).is_none() {
            if !self.is_leader {
                error_counters.account_not_found_validator += 1;
            } else {
                error_counters.account_not_found_leader += 1;
            }
            if FinPlanState::check_id(&tx.program_id) {
                use fin_plan_instruction::Instruction;
                if let Some(Instruction::NewVote(_vote)) = tx.instruction() {
                    error_counters.account_not_found_vote += 1;
                }
            }
            Err(TransactionProcessorError::AccountNotFound)
        } else if accounts.get(&tx.keys[0]).unwrap().tokens < tx.fee {
            Err(TransactionProcessorError::InsufficientFundsForFee)
        } else {
            let mut called_accounts: Vec<Account> = tx
                .keys
                .iter()
                .map(|key| accounts.get(key).cloned().unwrap_or_default())
                .collect();
            self.reserve_signature_with_last_id(&tx.signature, &tx.last_id)?;
            called_accounts[0].tokens -= tx.fee;
            Ok(called_accounts)
        }
    }

    fn load_accounts(
        &self,
        txs: &[Transaction],
        accounts: &HashMap<Pubkey, Account>,
        error_counters: &mut ErrorCounters,
    ) -> Vec<Result<Vec<Account>>> {
        txs.iter()
            .map(|tx| self.load_account(tx, accounts, error_counters))
            .collect()
    }

    pub fn verify_transaction(
        tx: &Transaction,
        pre_program_id: &Pubkey,
        pre_tokens: i64,
        account: &Account,
    ) -> Result<()> {

        if !((*pre_program_id == account.program_id)
            || (SystemProgram::check_id(&tx.program_id)
                && SystemProgram::check_id(&pre_program_id)))
        {
            return Err(TransactionProcessorError::ModifiedContractId);
        }

        if tx.program_id != account.program_id && pre_tokens > account.tokens {
            return Err(TransactionProcessorError::ExternalAccountTokenSpend);
        }
        if account.tokens < 0 {
            return Err(TransactionProcessorError::ResultWithNegativeTokens);
        }
        Ok(())
    }

    fn loaded_contract(&self, tx: &Transaction, accounts: &mut [Account]) -> bool {
        let loaded_contracts = self.loaded_contracts.write().unwrap();
        match loaded_contracts.get(&tx.program_id) {
            Some(dc) => {
                let mut infos: Vec<_> = (&tx.keys)
                    .into_iter()
                    .zip(accounts)
                    .map(|(key, account)| KeyedAccount { key, account })
                    .collect();

                dc.call(&mut infos, &tx.userdata);
                true
            }
            None => false,
        }
    }


    fn execute_transaction(&self, tx: &Transaction, accounts: &mut [Account]) -> Result<()> {
        let pre_total: i64 = accounts.iter().map(|a| a.tokens).sum();
        let pre_data: Vec<_> = accounts
            .iter_mut()
            .map(|a| (a.program_id, a.tokens))
            .collect();

  
        if SystemProgram::check_id(&tx.program_id) {
            SystemProgram::process_transaction(&tx, accounts, &self.loaded_contracts)
        } else if FinPlanState::check_id(&tx.program_id) {

            if FinPlanState::process_transaction(&tx, accounts).is_err() {
                return Err(TransactionProcessorError::ProgramRuntimeError);
            }
        } else if StorageProgram::check_id(&tx.program_id) {
            if StorageProgram::process_transaction(&tx, accounts).is_err() {
                return Err(TransactionProcessorError::ProgramRuntimeError);
            }
        } else if TicTacToeProgram::check_id(&tx.program_id) {
            if TicTacToeProgram::process_transaction(&tx, accounts).is_err() {
                return Err(TransactionProcessorError::ProgramRuntimeError);
            }
        } else if TicTacToeDashboardProgram::check_id(&tx.program_id) {
            if TicTacToeDashboardProgram::process_transaction(&tx, accounts).is_err() {
                return Err(TransactionProcessorError::ProgramRuntimeError);
            }
        } else if self.loaded_contract(&tx, accounts) {
        } else {
            return Err(TransactionProcessorError::UnknownContractId);
        }

        for ((pre_program_id, pre_tokens), post_account) in pre_data.iter().zip(accounts.iter()) {
            Self::verify_transaction(&tx, pre_program_id, *pre_tokens, post_account)?;
        }

        let post_total: i64 = accounts.iter().map(|a| a.tokens).sum();
        if pre_total != post_total {
            Err(TransactionProcessorError::UnbalancedTransaction)
        } else {
            Ok(())
        }
    }

    pub fn store_accounts(
        txs: &[Transaction],
        res: &[Result<()>],
        loaded: &[Result<Vec<Account>>],
        accounts: &mut HashMap<Pubkey, Account>,
    ) {
        for (i, racc) in loaded.iter().enumerate() {
            if res[i].is_err() || racc.is_err() {
                continue;
            }

            let tx = &txs[i];
            let acc = racc.as_ref().unwrap();
            for (key, account) in tx.keys.iter().zip(acc.iter()) {
                if account.tokens == 0 {
                    accounts.remove(&key);
                } else {
                    *accounts.entry(*key).or_insert_with(Account::default) = account.clone();
                    assert_eq!(accounts.get(key).unwrap().tokens, account.tokens);
                }
            }
        }
    }

    #[must_use]
    pub fn process_transactions(&self, txs: &[Transaction]) -> Vec<Result<()>> {
        debug!("processing transactions: {}", txs.len());

        let mut accounts = self.accounts.write().unwrap();
        let txs_len = txs.len();
        let mut error_counters = ErrorCounters::default();
        let now = Instant::now();
        let mut loaded_accounts = self.load_accounts(&txs, &accounts, &mut error_counters);
        let load_elapsed = now.elapsed();
        let now = Instant::now();

        let res: Vec<_> = loaded_accounts
            .iter_mut()
            .zip(txs.iter())
            .map(|(acc, tx)| match acc {
                Err(e) => Err(e.clone()),
                Ok(ref mut accounts) => self.execute_transaction(tx, accounts),
            }).collect();
        let execution_elapsed = now.elapsed();
        let now = Instant::now();
        Self::store_accounts(&txs, &res, &loaded_accounts, &mut accounts);
        self.update_transaction_statuses(&txs, &res);
        let write_elapsed = now.elapsed();
        debug!(
            "load: {}us execution: {}us write: {}us txs_len={}",
            duration_as_us(&load_elapsed),
            duration_as_us(&execution_elapsed),
            duration_as_us(&write_elapsed),
            txs_len
        );
        let mut tx_count = 0;
        let mut err_count = 0;
        for r in &res {
            if r.is_ok() {
                tx_count += 1;
            } else {
                if err_count == 0 {
                    debug!("tx error: {:?}", r);
                }
                err_count += 1;
            }
        }
        if err_count > 0 {
            info!("{} errors of {} txs", err_count, err_count + tx_count);
            if !self.is_leader {
                inc_new_counter_info!("transaction_processor-process_transactions_err-validator", err_count);
                inc_new_counter_info!(
                    "transaction_processor-appy_debits-account_not_found-validator",
                    error_counters.account_not_found_validator
                );
            } else {
                inc_new_counter_info!("transaction_processor-process_transactions_err-leader", err_count);
                inc_new_counter_info!(
                    "transaction_processor-appy_debits-account_not_found-leader",
                    error_counters.account_not_found_leader
                );
                inc_new_counter_info!(
                    "transaction_processor-appy_debits-vote_account_not_found",
                    error_counters.account_not_found_vote
                );
            }
        }
        let cur_tx_count = self.transaction_count.load(Ordering::Relaxed);
        if ((cur_tx_count + tx_count) & !(262_144 - 1)) > cur_tx_count & !(262_144 - 1) {
            info!("accounts.len: {}", accounts.len());
        }
        self.transaction_count
            .fetch_add(tx_count, Ordering::Relaxed);
        res
    }

    pub fn process_entry(&self, entry: &Entry) -> Result<()> {
        if !entry.transactions.is_empty() {
            for result in self.process_transactions(&entry.transactions) {
                result?;
            }
        }
        self.register_entry_id(&entry.id);
        Ok(())
    }

    fn process_entries_tail(
        &self,
        entries: Vec<Entry>,
        tail: &mut Vec<Entry>,
        tail_idx: &mut usize,
    ) -> Result<u64> {
        let mut entry_count = 0;

        for entry in entries {
            if tail.len() > *tail_idx {
                tail[*tail_idx] = entry.clone();
            } else {
                tail.push(entry.clone());
            }
            *tail_idx = (*tail_idx + 1) % WINDOW_SIZE as usize;

            entry_count += 1;
            self.process_entry(&entry)?;
        }

        Ok(entry_count)
    }

    pub fn process_entries(&self, entries: &[Entry]) -> Result<()> {
        for entry in entries {
            self.process_entry(&entry)?;
        }
        Ok(())
    }

    fn process_blocks<I>(
        &self,
        start_hash: Hash,
        entries: I,
        tail: &mut Vec<Entry>,
        tail_idx: &mut usize,
    ) -> Result<u64>
    where
        I: IntoIterator<Item = Entry>,
    {

        let mut entry_count = *tail_idx as u64;
        let mut id = start_hash;
        for block in &entries.into_iter().chunks(VERIFY_BLOCK_SIZE) {
            let block: Vec<_> = block.collect();
            if !block.verify(&id) {
                warn!("Ledger proof of history failed at entry: {}", entry_count);
                return Err(TransactionProcessorError::LedgerVerificationFailed);
            }
            id = block.last().unwrap().id;
            entry_count += self.process_entries_tail(block, tail, tail_idx)?;
        }
        Ok(entry_count)
    }


    pub fn process_ledger<I>(&self, entries: I) -> Result<(u64, Vec<Entry>)>
    where
        I: IntoIterator<Item = Entry>,
    {
        let mut entries = entries.into_iter();

        let entry0 = entries.next().expect("invalid ledger: empty");


        let entry1 = entries
            .next()
            .expect("invalid ledger: need at least 2 entries");
        {
            let tx = &entry1.transactions[0];
            assert!(SystemProgram::check_id(&tx.program_id), "Invalid ledger");
            let instruction: SystemProgram = deserialize(&tx.userdata).unwrap();
            let deposit = if let SystemProgram::Move { tokens } = instruction {
                Some(tokens)
            } else {
                None
            }.expect("invalid ledger, needs to start with a contract");
            {
                let mut accounts = self.accounts.write().unwrap();
                let account = accounts.entry(tx.keys[0]).or_insert_with(Account::default);
                account.tokens += deposit;
                trace!("applied genesis payment {:?} => {:?}", deposit, account);
            }
        }
        self.register_entry_id(&entry0.id);
        self.register_entry_id(&entry1.id);
        let entry1_id = entry1.id;

        let mut tail = Vec::with_capacity(WINDOW_SIZE as usize);
        tail.push(entry0);
        tail.push(entry1);
        let mut tail_idx = 2;
        let entry_count = self.process_blocks(entry1_id, entries, &mut tail, &mut tail_idx)?;

        if tail.len() == WINDOW_SIZE as usize {
            tail.rotate_left(tail_idx)
        }

        Ok((entry_count, tail))
    }


    pub fn transfer(
        &self,
        n: i64,
        keypair: &Keypair,
        to: Pubkey,
        last_id: Hash,
    ) -> Result<Signature> {
        let tx = Transaction::system_new(keypair, to, n, last_id);
        let signature = tx.signature;
        self.process_transaction(&tx).map(|_| signature)
    }

    pub fn read_balance(account: &Account) -> i64 {
        if SystemProgram::check_id(&account.program_id) {
            SystemProgram::get_balance(account)
        } else if FinPlanState::check_id(&account.program_id) {
            FinPlanState::get_balance(account)
        } else {
            account.tokens
        }
    }

    pub fn get_balance(&self, pubkey: &Pubkey) -> i64 {
        self.get_account(pubkey)
            .map(|x| Self::read_balance(&x))
            .unwrap_or(0)
    }

    pub fn get_account(&self, pubkey: &Pubkey) -> Option<Account> {
        let accounts = self
            .accounts
            .read()
            .expect("'accounts' read lock in get_balance");
        accounts.get(pubkey).cloned()
    }

    pub fn transaction_count(&self) -> usize {
        self.transaction_count.load(Ordering::Relaxed)
    }

    pub fn get_signature_status(&self, signature: &Signature) -> Result<()> {
        let last_ids_sigs = self.last_ids_sigs.read().unwrap();
        for (_hash, (signatures, _)) in last_ids_sigs.iter() {
            if let Some(res) = signatures.get(signature) {
                return res.clone();
            }
        }
        Err(TransactionProcessorError::SignatureNotFound)
    }

    pub fn has_signature(&self, signature: &Signature) -> bool {
        self.get_signature_status(signature) != Err(TransactionProcessorError::SignatureNotFound)
    }


    pub fn hash_internal_state(&self) -> Hash {
        let mut ordered_accounts = BTreeMap::new();
        for (pubkey, account) in self.accounts.read().unwrap().iter() {
            ordered_accounts.insert(*pubkey, account.clone());
        }
        hash(&serialize(&ordered_accounts).unwrap())
    }

    pub fn finality(&self) -> usize {
        self.finality_time.load(Ordering::Relaxed)
    }

    pub fn set_finality(&self, finality: usize) {
        self.finality_time.store(finality, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::serialize;
    use entry::next_entry;
    use entry::Entry;
    use entry_writer::{self, EntryWriter};
    use hash::hash;
    use ledger;
    use logger;
    use signature::{GenKeys, KeypairUtil};
    use std;
    use std::io::{BufReader, Cursor, Seek, SeekFrom};

    #[test]
    fn test_transaction_processor_new() {
        let mint = Mint::new(10_000);
        let transaction_processor = TransactionProcessor::new(&mint);
        assert_eq!(transaction_processor.get_balance(&mint.pubkey()), 10_000);
    }

    #[test]
    fn test_two_payments_to_one_party() {
        let mint = Mint::new(10_000);
        let pubkey = Keypair::new().pubkey();
        let transaction_processor = TransactionProcessor::new(&mint);
        assert_eq!(transaction_processor.last_id(), mint.last_id());

        transaction_processor.transfer(1_000, &mint.keypair(), pubkey, mint.last_id())
            .unwrap();
        assert_eq!(transaction_processor.get_balance(&pubkey), 1_000);

        transaction_processor.transfer(500, &mint.keypair(), pubkey, mint.last_id())
            .unwrap();
        assert_eq!(transaction_processor.get_balance(&pubkey), 1_500);
        assert_eq!(transaction_processor.transaction_count(), 2);
    }

    #[test]
    fn test_negative_tokens() {
        logger::setup();
        let mint = Mint::new(1);
        let pubkey = Keypair::new().pubkey();
        let transaction_processor = TransactionProcessor::new(&mint);
        let res = transaction_processor.transfer(-1, &mint.keypair(), pubkey, mint.last_id());
        println!("{:?}", transaction_processor.get_account(&pubkey));
        assert_matches!(res, Err(TransactionProcessorError::ResultWithNegativeTokens));
        assert_eq!(transaction_processor.transaction_count(), 0);
    }


    #[test]
    fn test_detect_failed_duplicate_transactions_issue_1157() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let dest = Keypair::new();


        let tx = Transaction::system_create(
            &mint.keypair(),
            dest.pubkey(),
            mint.last_id(),
            2,
            0,
            Pubkey::default(),
            1,
        );
        let signature = tx.signature;
        assert!(!transaction_processor.has_signature(&signature));
        let res = transaction_processor.process_transaction(&tx);


        assert!(!res.is_ok());
        assert!(transaction_processor.has_signature(&signature));
        assert_matches!(
            transaction_processor.get_signature_status(&signature),
            Err(TransactionProcessorError::ResultWithNegativeTokens)
        );


        assert_eq!(transaction_processor.get_balance(&dest.pubkey()), 0);

    }

    #[test]
    fn test_account_not_found() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let keypair = Keypair::new();
        assert_eq!(
            transaction_processor.transfer(1, &keypair, mint.pubkey(), mint.last_id()),
            Err(TransactionProcessorError::AccountNotFound)
        );
        assert_eq!(transaction_processor.transaction_count(), 0);
    }

    #[test]
    fn test_insufficient_funds() {
        let mint = Mint::new(11_000);
        let transaction_processor = TransactionProcessor::new(&mint);
        let pubkey = Keypair::new().pubkey();
        transaction_processor.transfer(1_000, &mint.keypair(), pubkey, mint.last_id())
            .unwrap();
        assert_eq!(transaction_processor.transaction_count(), 1);
        assert_eq!(transaction_processor.get_balance(&pubkey), 1_000);
        assert_matches!(
            transaction_processor.transfer(10_001, &mint.keypair(), pubkey, mint.last_id()),
            Err(TransactionProcessorError::ResultWithNegativeTokens)
        );
        assert_eq!(transaction_processor.transaction_count(), 1);

        let mint_pubkey = mint.keypair().pubkey();
        assert_eq!(transaction_processor.get_balance(&mint_pubkey), 10_000);
        assert_eq!(transaction_processor.get_balance(&pubkey), 1_000);
    }

    #[test]
    fn test_transfer_to_newb() {
        let mint = Mint::new(10_000);
        let transaction_processor = TransactionProcessor::new(&mint);
        let pubkey = Keypair::new().pubkey();
        transaction_processor.transfer(500, &mint.keypair(), pubkey, mint.last_id())
            .unwrap();
        assert_eq!(transaction_processor.get_balance(&pubkey), 500);
    }

    #[test]
    fn test_duplicate_transaction_signature() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let signature = Signature::default();
        assert!(
            transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id())
                .is_ok()
        );
        assert_eq!(
            transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id()),
            Err(TransactionProcessorError::DuplicateSignature)
        );
    }

    #[test]
    fn test_clear_signatures() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let signature = Signature::default();
        transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id())
            .unwrap();
        transaction_processor.clear_signatures();
        assert!(
            transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id())
                .is_ok()
        );
    }

    #[test]
    fn test_get_signature_status() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let signature = Signature::default();
        transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id())
            .expect("reserve signature");
        assert!(transaction_processor.get_signature_status(&signature).is_ok());
    }

    #[test]
    fn test_has_signature() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let signature = Signature::default();
        transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id())
            .expect("reserve signature");
        assert!(transaction_processor.has_signature(&signature));
    }

    #[test]
    fn test_reject_old_last_id() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let signature = Signature::default();
        for i in 0..MAX_ENTRY_IDS {
            let last_id = hash(&serialize(&i).unwrap()); // Unique hash
            transaction_processor.register_entry_id(&last_id);
        }

        assert_eq!(
            transaction_processor.reserve_signature_with_last_id(&signature, &mint.last_id()),
            Err(TransactionProcessorError::LastIdNotFound)
        );
    }

    #[test]
    fn test_count_valid_ids() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let ids: Vec<_> = (0..MAX_ENTRY_IDS)
            .map(|i| {
                let last_id = hash(&serialize(&i).unwrap()); // Unique hash
                transaction_processor.register_entry_id(&last_id);
                last_id
            }).collect();
        assert_eq!(transaction_processor.count_valid_ids(&[]).len(), 0);
        assert_eq!(transaction_processor.count_valid_ids(&[mint.last_id()]).len(), 0);
        for (i, id) in transaction_processor.count_valid_ids(&ids).iter().enumerate() {
            assert_eq!(id.0, i);
        }
    }

    #[test]
    fn test_debits_before_credits() {
        let mint = Mint::new(2);
        let transaction_processor = TransactionProcessor::new(&mint);
        let keypair = Keypair::new();
        let tx0 = Transaction::system_new(&mint.keypair(), keypair.pubkey(), 2, mint.last_id());
        let tx1 = Transaction::system_new(&keypair, mint.pubkey(), 1, mint.last_id());
        let txs = vec![tx0, tx1];
        let results = transaction_processor.process_transactions(&txs);
        assert!(results[1].is_err());

        // Assert bad transactions aren't counted.
        assert_eq!(transaction_processor.transaction_count(), 1);
    }

    #[test]
    fn test_process_empty_entry_is_registered() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let keypair = Keypair::new();
        let entry = next_entry(&mint.last_id(), 1, vec![]);
        let tx = Transaction::system_new(&mint.keypair(), keypair.pubkey(), 1, entry.id);

        // First, ensure the TX is rejected because of the unregistered last ID
        assert_eq!(
            transaction_processor.process_transaction(&tx),
            Err(TransactionProcessorError::LastIdNotFound)
        );

        // Now ensure the TX is accepted despite pointing to the ID of an empty entry.
        transaction_processor.process_entries(&[entry]).unwrap();
        assert!(transaction_processor.process_transaction(&tx).is_ok());
    }

    #[test]
    fn test_process_genesis() {
        let mint = Mint::new(1);
        let genesis = mint.create_entries();
        let transaction_processor = TransactionProcessor::default();
        transaction_processor.process_ledger(genesis).unwrap();
        assert_eq!(transaction_processor.get_balance(&mint.pubkey()), 1);
    }

    fn create_sample_block_with_next_entries_using_keypairs(
        mint: &Mint,
        keypairs: &[Keypair],
    ) -> impl Iterator<Item = Entry> {
        let hash = mint.last_id();
        let transactions: Vec<_> = keypairs
            .iter()
            .map(|keypair| Transaction::system_new(&mint.keypair(), keypair.pubkey(), 1, hash))
            .collect();
        let entries = ledger::next_entries(&hash, 0, transactions);
        entries.into_iter()
    }

    fn create_sample_block(mint: &Mint, length: usize) -> impl Iterator<Item = Entry> {
        let mut entries = Vec::with_capacity(length);
        let mut hash = mint.last_id();
        let mut num_hashes = 0;
        for _ in 0..length {
            let keypair = Keypair::new();
            let tx = Transaction::system_new(&mint.keypair(), keypair.pubkey(), 1, hash);
            let entry = Entry::new_mut(&mut hash, &mut num_hashes, vec![tx]);
            entries.push(entry);
        }
        entries.into_iter()
    }

    fn create_sample_ledger(length: usize) -> (impl Iterator<Item = Entry>, Pubkey) {
        let mint = Mint::new(1 + length as i64);
        let genesis = mint.create_entries();
        let block = create_sample_block(&mint, length);
        (genesis.into_iter().chain(block), mint.pubkey())
    }

    fn create_sample_ledger_with_mint_and_keypairs(
        mint: &Mint,
        keypairs: &[Keypair],
    ) -> impl Iterator<Item = Entry> {
        let genesis = mint.create_entries();
        let block = create_sample_block_with_next_entries_using_keypairs(mint, keypairs);
        genesis.into_iter().chain(block)
    }

    #[test]
    fn test_process_ledger() {
        let (ledger, pubkey) = create_sample_ledger(1);
        let (ledger, dup) = ledger.tee();
        let transaction_processor = TransactionProcessor::default();
        let (ledger_height, tail) = transaction_processor.process_ledger(ledger).unwrap();
        assert_eq!(transaction_processor.get_balance(&pubkey), 1);
        assert_eq!(ledger_height, 3);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail, dup.collect_vec());
        let last_entry = &tail[tail.len() - 1];
        assert_eq!(transaction_processor.last_id(), last_entry.id);
    }

    #[test]
    fn test_process_ledger_around_window_size() {


        let window_size = WINDOW_SIZE as usize;
        for entry_count in window_size - 3..window_size + 2 {
            let (ledger, pubkey) = create_sample_ledger(entry_count);
            let transaction_processor = TransactionProcessor::default();
            let (ledger_height, tail) = transaction_processor.process_ledger(ledger).unwrap();
            assert_eq!(transaction_processor.get_balance(&pubkey), 1);
            assert_eq!(ledger_height, entry_count as u64 + 2);
            assert!(tail.len() <= window_size);
            let last_entry = &tail[tail.len() - 1];
            assert_eq!(transaction_processor.last_id(), last_entry.id);
        }
    }

    fn to_file_iter(entries: impl Iterator<Item = Entry>) -> impl Iterator<Item = Entry> {
        let mut file = Cursor::new(vec![]);
        EntryWriter::write_entries(&mut file, entries).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();

        let reader = BufReader::new(file);
        entry_writer::read_entries(reader).map(|x| x.unwrap())
    }

    #[test]
    fn test_process_ledger_from_file() {
        let (ledger, pubkey) = create_sample_ledger(1);
        let ledger = to_file_iter(ledger);

        let transaction_processor = TransactionProcessor::default();
        transaction_processor.process_ledger(ledger).unwrap();
        assert_eq!(transaction_processor.get_balance(&pubkey), 1);
    }

    #[test]
    fn test_process_ledger_from_files() {
        let mint = Mint::new(2);
        let genesis = to_file_iter(mint.create_entries().into_iter());
        let block = to_file_iter(create_sample_block(&mint, 1));

        let transaction_processor = TransactionProcessor::default();
        transaction_processor.process_ledger(genesis.chain(block)).unwrap();
        assert_eq!(transaction_processor.get_balance(&mint.pubkey()), 1);
    }

    #[test]
    fn test_new_default() {
        let def_transaction_processor = TransactionProcessor::default();
        assert!(def_transaction_processor.is_leader);
        let leader_transaction_processor = TransactionProcessor::new_default(true);
        assert!(leader_transaction_processor.is_leader);
        let validator_transaction_processor = TransactionProcessor::new_default(false);
        assert!(!validator_transaction_processor.is_leader);
    }
    #[test]
    fn test_hash_internal_state() {
        let mint = Mint::new(2_000);
        let seed = [0u8; 32];
        let mut rnd = GenKeys::new(seed);
        let keypairs = rnd.gen_n_keypairs(5);
        let ledger0 = create_sample_ledger_with_mint_and_keypairs(&mint, &keypairs);
        let ledger1 = create_sample_ledger_with_mint_and_keypairs(&mint, &keypairs);

        let transaction_processor0 = TransactionProcessor::default();
        transaction_processor0.process_ledger(ledger0).unwrap();
        let transaction_processor1 = TransactionProcessor::default();
        transaction_processor1.process_ledger(ledger1).unwrap();

        let initial_state = transaction_processor0.hash_internal_state();

        assert_eq!(transaction_processor1.hash_internal_state(), initial_state);

        let pubkey = keypairs[0].pubkey();
        transaction_processor0
            .transfer(1_000, &mint.keypair(), pubkey, mint.last_id())
            .unwrap();
        assert_ne!(transaction_processor0.hash_internal_state(), initial_state);
        transaction_processor1
            .transfer(1_000, &mint.keypair(), pubkey, mint.last_id())
            .unwrap();
        assert_eq!(transaction_processor0.hash_internal_state(), transaction_processor1.hash_internal_state());
    }
    #[test]
    fn test_finality() {
        let def_transaction_processor = TransactionProcessor::default();
        assert_eq!(def_transaction_processor.finality(), std::usize::MAX);
        def_transaction_processor.set_finality(90);
        assert_eq!(def_transaction_processor.finality(), 90);
    }

    #[test]
    fn test_storage_tx() {
        let mint = Mint::new(1);
        let transaction_processor = TransactionProcessor::new(&mint);
        let tx = Transaction::new(
            &mint.keypair(),
            &[],
            StorageProgram::id(),
            vec![], // <--- attack! Panic on bad userdata?
            mint.last_id(),
            0,
        );
        assert!(transaction_processor.process_transaction(&tx).is_err());
    }
}
