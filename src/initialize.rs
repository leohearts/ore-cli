use ore::TREASURY_ADDRESS;
use solana_program::instruction::Instruction;
use solana_sdk::signature::Signer;
use solana_program::pubkey::Pubkey;
use solana_program::instruction::AccountMeta;
use crate::Miner;

impl Miner {
    pub async fn initialize(&self) {
        // Return early if program is initialized
        let signer = self.signer();
        let client = self.rpc_client.clone();
        if client.get_account(&TREASURY_ADDRESS).await.is_ok() {
            return;
        }

        // Reset epoch, if needed
        let treasury = get_treasury(&self.rpc_client).await;
        let clock = get_clock_account(&self.rpc_client).await;
        let threshold = treasury.last_reset_at.saturating_add(EPOCH_DURATION);
        if clock.unix_timestamp.ge(&threshold) {
            // There are a lot of miners right now, so randomly select into submitting tx
            if rng.gen_range(0..RESET_ODDS).eq(&0) {
                println!("Sending epoch reset transaction...");
                let cu_limit_ix =
                    ComputeBudgetInstruction::set_compute_unit_limit(CU_LIMIT_RESET);
                let cu_price_ix =
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee);
                let reset_ix = ore::instruction::reset(signer.pubkey());
                self.send_and_confirm(&[cu_limit_ix, cu_price_ix, reset_ix], false, true)
                    .await
                    .ok();
            }
        }
        println!("Init");
        // Sign and send transaction.
        let mut ix = ore::instruction::initialize(signer.pubkey());
        self.send_and_confirm(&[ix], true, false)
            .await
            .expect("Transaction failed");
    }
}
