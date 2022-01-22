//! Program state processor
use std::collections::HashMap;

use crate::{
    error::LotteryError,
    instruction::LotteryInstruction,
    state::{LotteryData, LotteryResultData, TicketData},
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    native_token::{lamports_to_sol, sol_to_lamports},
    program::invoke,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};
use spl_token::{state::Mint, ui_amount_to_amount};

use switchboard_program::VrfAccount;

// Sollotto program_id      Add here
solana_program::declare_id!(" ");

/// Checks that the supplied program ID is the correct
pub fn check_program_account(program_id: &Pubkey) -> ProgramResult {
    if program_id != &id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

/// Program state handler.
pub struct Processor;
impl<'a> Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &'a[AccountInfo<'a>],
        instruction_data: &[u8],
    ) -> ProgramResult {
        check_program_account(program_id)?;

        let instruction = LotteryInstruction::unpack(instruction_data)?;
        match instruction {
            LotteryInstruction::InitLottery {
                lottery_id,
                
                holding_wallet,
                rewards_wallet,
               
                randomness_account,
            } => {
                msg!("Instruction: InitLottery");
                Self::process_init_lottery(
                    program_id,
                    accounts,
                    lottery_id,
                    holding_wallet,
                    rewards_wallet,
                    randomness_account,
                )
            }

            LotteryInstruction::PurchaseTicket {
                user_wallet_pk,
                ticket_number_arr,
            } => {
                msg!("Instruction: PurchaseTicket");
                Self::process_ticket_purchase(
                    program_id,
                    accounts,
                    user_wallet_pk,
                    ticket_number_arr,
                )
            }

            LotteryInstruction::StoreWinningNumbers {} => {
                msg!("Instruction: store winning numbers");
                Self::process_store_winning_numbers(program_id, accounts)
            }

            LotteryInstruction::RewardWinners {} => {
                msg!("Instruction: reward winners");
                Self::process_reward_winners(program_id, accounts)
            }

        }
    }

    pub fn process_init_lottery(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        lottery_id: u32,
        holding_wallet: Pubkey,
        rewards_wallet: Pubkey,
        randomness_account: Pubkey,
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();

        // lottery data account
        let lottery_data_account = next_account_info(accounts_iter)?;

        // Check if program owns data account
        if lottery_data_account.owner != program_id {
            msg!("Lottery Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }

        if !lottery_data_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let rent = &Rent::from_account_info(next_account_info(accounts_iter)?)?;
        if !rent.is_exempt(
            lottery_data_account.lamports(),
            lottery_data_account.data_len(),
        ) {
            return Err(LotteryError::NotRentExempt.into());
        }

        // Add data to account
        let mut lottery_data = LotteryData::unpack_unchecked(&lottery_data_account.data.borrow())?;
        if lottery_data.is_initialized {
            msg!("Lottery data account already initialized");
            return Err(LotteryError::Initialized.into());
        }

        lottery_data.is_initialized = true;
        lottery_data.lottery_id = lottery_id;
        lottery_data.holding_wallet = holding_wallet;
        lottery_data.rewards_wallet = rewards_wallet;
        lottery_data.randomness_account = randomness_account;
        lottery_data.total_registrations = 0;
        LotteryData::pack(lottery_data, &mut lottery_data_account.data.borrow_mut())?;

        msg!("Data stored");

        Ok(())
    }

    pub fn process_ticket_purchase(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        charity: Pubkey,
        user_wallet_pk: Pubkey,
        ticket_number_arr: [u8; 6],
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        let lottery_data_account = next_account_info(accounts_iter)?;
        let ticket_data_account = next_account_info(accounts_iter)?;
        let user_funding_account = next_account_info(accounts_iter)?;
        let holding_wallet_account = next_account_info(accounts_iter)?;
        let user_lifetime_ticket_account = next_account_info(accounts_iter)?;
        let lifetime_ticket_owner_account = next_account_info(accounts_iter)?;
        let lifetime_ticket_mint_account = next_account_info(accounts_iter)?;
        let rent = &Rent::from_account_info(next_account_info(accounts_iter)?)?;
        let system_program_info = next_account_info(accounts_iter)?;
        let spl_token_info = next_account_info(accounts_iter)?;

        if lottery_data_account.owner != program_id {
            msg!("Lottery Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }
        if ticket_data_account.owner != program_id {
            msg!("Ticket Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }

        if !lottery_data_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        if !user_funding_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        if !lifetime_ticket_owner_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut lottery_data = LotteryData::unpack_unchecked(&lottery_data_account.data.borrow())?;
        //Check if lottery initisalised
        if !lottery_data.is_initialized {
            msg!("Ticket data account is not initialized");
            return Err(LotteryError::NotInitialized.into());
        }
        if lottery_data.is_finaled {
            msg!("Lottery data account already finaled");
            return Err(LotteryError::IsFinaled.into());
        }

        if *holding_wallet_account.key != lottery_data.holding_wallet {
            msg!("Missing holding wallet");
            return Err(LotteryError::InvalidSollottoAccount.into());
        }

        if user_funding_account.lamports() < sol_to_lamports(0.1) {
            msg!("User cannot pay for ticket");
            return Err(ProgramError::InsufficientFunds);
        }

        if !rent.is_exempt(
            lottery_data_account.lamports(),
            lottery_data_account.data_len(),
        ) {
            return Err(LotteryError::NotRentExempt.into());
        }

        if !rent.is_exempt(
            ticket_data_account.lamports(),
            ticket_data_account.data_len(),
        ) {
            return Err(LotteryError::NotRentExempt.into());
        }

        let mut ticket_data = TicketData::unpack_unchecked(&ticket_data_account.data.borrow())?;
        if ticket_data.is_purchased {
            msg!("Ticket data account already purchased");
            return Err(LotteryError::AlreadyPurchased.into());
        }

        for i in 0..5 {
            if ticket_number_arr[i] < 1 || ticket_number_arr[i] > 69 {
                msg!("Invalid value for one of from 1 to 5 number");
                return Err(LotteryError::InvalidNumber.into());
            }
        }
        if ticket_number_arr[5] < 1 || ticket_number_arr[5] > 29 {
            msg!("Invalid value for 6 number");
            return Err(LotteryError::InvalidNumber.into());
        }

        ticket_data.is_purchased = true;
        ticket_data.user_wallet_pk = user_wallet_pk;
        ticket_data.ticket_number_arr = ticket_number_arr;

        lottery_data.total_registrations += 1;
        
      

        // Transfer 0.1 SOL into holding wallet from user_wallet
        //We have to transfer SOL/USD acc to the live price feeds
        let ticket_price = sol_to_lamports(0.1);
        invoke(
            &system_instruction::transfer(
                &user_wallet_pk,
                &lottery_data.holding_wallet,
                ticket_price,
            ),
            &[
                user_funding_account.clone(),
                holding_wallet_account.clone(),
                system_program_info.clone(),
            ],
        )?;

        // Mint 1.0 Lifetime Ticket Token to user
        //Need to mint the NFT here
        let decimals = Mint::unpack(&lifetime_ticket_mint_account.data.borrow())?.decimals;
        let amount = ui_amount_to_amount(1.0, decimals);
        invoke(
            &spl_token::instruction::mint_to(
                &spl_token::id(),
                lifetime_ticket_mint_account.key,
                user_lifetime_ticket_account.key,
                lifetime_ticket_owner_account.key,
                &[],
                amount,
            )
            .unwrap(),
            &[
                spl_token_info.clone(),
                lifetime_ticket_mint_account.clone(),
                user_lifetime_ticket_account.clone(),
                lifetime_ticket_owner_account.clone(),
            ],
        )?;

        lottery_data.prize_pool_amount += ticket_price;

        TicketData::pack(ticket_data, &mut ticket_data_account.data.borrow_mut())?;
        LotteryData::pack(lottery_data, &mut lottery_data_account.data.borrow_mut())?;

        Ok(())
    }

    pub fn process_store_winning_numbers(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'a>],
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        let lottery_data_account = next_account_info(accounts_iter)?;

        if lottery_data_account.owner != program_id {
            msg!("Lottery Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }

        if !lottery_data_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut lottery_data = LotteryData::unpack_unchecked(&lottery_data_account.data.borrow())?;
        if !lottery_data.is_initialized {
            msg!("Lottery Data account is not initialized");
            return Err(LotteryError::NotInitialized.into());
        }
        if lottery_data.is_finaled {
            msg!("Lottery Data account already finaled");
            return Err(LotteryError::IsFinaled.into());
        }

        // if *vrf_account_info.key != lottery_data.randomness_account {
        //     return Err(LotteryError::InvalidSollottoAccount.into());
        // }

        let vrf_account_info = next_account_info(accounts_iter)?;
        let vrf_account = VrfAccount::new(vrf_account_info)?;
        let random_numbers = vrf_account.get_verified_randomness()?;
        // drop(vrf_account);
        if random_numbers.len() < 6 {
            return Err(LotteryError::InvalidRandomResult.into());
        }

        let mut winning_numbers_arr: [u8; 6] = [0; 6];
        for i in 0..4 {
            winning_numbers_arr[i] = random_numbers[i] % 49 + 1;
        }
        winning_numbers_arr[5] = random_numbers[5] % 26 + 1;

        for i in 0..5 {
            if winning_numbers_arr[i] < 1 || winning_numbers_arr[i] > 69 {
                msg!("Invalid value for one of from 1 to 5 number");
                return Err(LotteryError::InvalidNumber.into());
            }
        }
        if winning_numbers_arr[5] < 1 || winning_numbers_arr[5] > 29 {
            msg!("Invalid value for 6 number");
            return Err(LotteryError::InvalidNumber.into());
        }

        lottery_data.is_finaled = true;
        lottery_data.winning_numbers = winning_numbers_arr;

        LotteryData::pack(lottery_data, &mut lottery_data_account.data.borrow_mut())?;

        Ok(())
    }

    pub fn process_reward_winners(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        let lottery_data_account = next_account_info(accounts_iter)?;
        let lottery_result_account = next_account_info(accounts_iter)?;
        let holding_wallet_account = next_account_info(accounts_iter)?;
        let rewards_wallet_account = next_account_info(accounts_iter)?;
        let system_program_info = next_account_info(accounts_iter)?;
        let participants_accounts = accounts_iter.as_slice();

        if lottery_data_account.owner != program_id {
            msg!("Lottery Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }
        if lottery_result_account.owner != program_id {
            msg!("Lottery Result Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }

        if !lottery_data_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        if !holding_wallet_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut lottery_data = LotteryData::unpack_unchecked(&lottery_data_account.data.borrow())?;
        if !lottery_data.is_initialized {
            msg!("Lottery Data account is not initialized");
            return Err(LotteryError::NotInitialized.into());
        }
        if !lottery_data.is_finaled {
            msg!("Lottery Data account have not winning numbers");
            return Err(LotteryError::NotFinaled.into());
        }

        // Check all sollotto keys
        if *holding_wallet_account.key != lottery_data.holding_wallet {
            return Err(LotteryError::InvalidSollottoAccount.into());
        }
        if *rewards_wallet_account.key != lottery_data.rewards_wallet {
            return Err(LotteryError::InvalidSollottoAccount.into());
        }
        

        if (participants_accounts.len() / 2) as u32 != lottery_data.total_registrations {
            msg!(
                "Invalid participants accounts size: {}",
                participants_accounts.len()
            );
            return Err(LotteryError::InvalidParticipantsAccounts.into());
        }
        msg!(
            "Participants accounts size: {}",
            participants_accounts.len()
        );

        for i in (0..participants_accounts.len()).step_by(2) {
            if participants_accounts[i].owner != program_id {
                msg!("Ticket Data account does not have the correct program id");
                return Err(ProgramError::IncorrectProgramId);
            }
            let data = TicketData::unpack_unchecked(&participants_accounts[i].data.borrow())?;
            if !data.is_purchased {
                msg!("Ticket data account is not purchased");
                return Err(LotteryError::NotInitialized.into());
            }

            if data.user_wallet_pk != *participants_accounts[i + 1].key {
                msg!("Bad user_wallet_pk in ticket data account");
                return Err(LotteryError::InvalidParticipantsAccounts.into());
            }
        }

        // Check winning numbers and find winner
        let mut winners6 = Vec::new();
        let mut winners5 = Vec::new();
        let mut winners4 = Vec::new();
        let mut winners3 = Vec::new();
        for i in (0..participants_accounts.len()).step_by(2) {
            let ticket = TicketData::unpack_unchecked(&participants_accounts[i].data.borrow())?;
            let mut matched: i32 = 0;
            for j in 0..5 {
                if ticket.ticket_number_arr[j] == lottery_data.winning_numbers[j] {
                    matched = matched + 1;
                }
            }
            if matched == 6 {
                msg!("Found winner {}", participants_accounts[i + 1].key);
                winners6.push(&participants_accounts[i + 1]);
            }
            if matched == 5 {
                msg!("Found tier 5 {}", participants_accounts[i + 1].key);
                winners5.push(&participants_accounts[i + 1]);
            }
            if matched == 4 {
                msg!("Found tier 4 {}", participants_accounts[i + 1].key);
                winners4.push(&participants_accounts[i + 1]);
            }
            if matched == 3 {
                msg!("Found tier 3 {}", participants_accounts[i + 1].key);
                winners3.push(&participants_accounts[i + 1]);
            }
        }

        if holding_wallet_account.lamports() < lottery_data.prize_pool_amount {
            msg!("Model 1 holding wallet InsufficientFunds error");
            return Err(ProgramError::InsufficientFunds);
        }

        let prize_pool = lamports_to_sol(lottery_data.prize_pool_amount);
        msg!("Prize pool in SOL: {}", prize_pool);

        

        // 7. 5% of the prize pool is transferred to the "Avalor" wallet address
        let solloto_reward = sol_to_lamports(prize_pool * 0.05);
        msg!("Solloto reward in lamports: {}", solloto_reward);
        // Transfer from lottery_data.holding_wallet to solloto_rewards_wallet
        invoke(
            &system_instruction::transfer(
                &lottery_data.holding_wallet,
                &lottery_data.rewards_wallet,
                solloto_reward,
            ),
            &[
                holding_wallet_account.clone(),
                rewards_wallet_account.clone(),
                system_program_info.clone(),
            ],
        )?;

        lottery_data.prize_pool_amount -= solloto_reward;

        

        // Process rewards
        let mut all_winners = Vec::new();
        let mut winner_rewards = Vec::new();
        let mut winners6_pool = prize_pool * 0.95;

        // 3 tiers
        let winner3_reward = sol_to_lamports(0.1);
        msg!("Winners(3 tier) number {}", winners3.len());
        msg!("Winner(3 tier) reward in lamports: {}", winner3_reward);
        for winner3 in winners3 {
            all_winners.push(winner3);
            winner_rewards.push(winner3_reward);
            winners6_pool = winners6_pool - 0.1;
        }

        // 4 tiers
        let winners4_pool = prize_pool * 0.05;
        let winner4_reward ;
        if winners4.len() != 0 {
            winner4_reward = sol_to_lamports(winners4_pool / winners4.len() as f64);
            winners6_pool = winners6_pool - winners4_pool;
        } else {
            winner4_reward = 0;
        }
        msg!("Winners(4 tier) number {}", winners4.len());
        msg!("Winner(4 tier) reward in lamports: {}", winner4_reward);
        for winner4 in winners4 {
            all_winners.push(winner4);
            winner_rewards.push(winner4_reward);
        }

        // 5 tiers
        let winners5_pool = prize_pool * 0.05;
        let winner5_reward ;
        if winners5.len() != 0 {
            winner5_reward = sol_to_lamports(winners5_pool / winners5.len() as f64);
            winners6_pool = winners6_pool - winners5_pool;
        } else {
            winner5_reward = 0;
        }
        msg!("Winners(5 tier) number {}", winners5.len());
        msg!("Winner(5 tier) reward in lamports: {}", winner5_reward);
        for winner5 in winners5 {
            all_winners.push(winner5);
            winner_rewards.push(winner5_reward);
        }

        // 6 tiers - perfect match
        let winner6_reward ;
        if winners6.len() != 0 {
            winner6_reward = sol_to_lamports(winners6_pool / winners6.len() as f64);
        } else {
            winner6_reward = 0;
        }
        msg!("Winners number {}", winners6.len());
        msg!("Winner reward in lamports: {}", winner6_reward);
        for winner6 in winners6 {
            all_winners.push(winner6);
            winner_rewards.push(winner6_reward);
        }

        for i in 0..all_winners.len() {
            // Transfer from lottery_data.holding_wallet to winner_wallet
            invoke(
                &system_instruction::transfer(
                    &lottery_data.holding_wallet,
                    &all_winners[i].key,
                    winner_rewards[i],
                ),
                &[
                    holding_wallet_account.clone(),
                    all_winners[i].clone(),
                    system_program_info.clone(),
                ],
            )?;

            lottery_data.prize_pool_amount -= winner_rewards[i];
        }

        // Create lottery result acc info
        let lottery_result = LotteryResultData {
            lottery_id: lottery_data.lottery_id,
            winning_numbers: lottery_data.winning_numbers,
        };

        // Clear lottery acc for new lottery
        lottery_data.is_initialized = false;
        lottery_data.is_finaled = false;
        lottery_data.winning_numbers = [0, 0];
        lottery_data.total_registrations = 0;
        lottery_data.lottery_id = 0;

        LotteryData::pack(lottery_data, &mut lottery_data_account.data.borrow_mut())?;
        LotteryResultData::pack(
            lottery_result,
            &mut lottery_result_account.data.borrow_mut(),
        )?;

        Ok(())
    }

    

    pub fn process_update_sollotto_wallets(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        holding_wallet: Pubkey,
        rewards_wallet: Pubkey,
        slot_holders_rewards_wallet: Pubkey,
        sollotto_labs_wallet: Pubkey,
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        let lottery_data_account = next_account_info(accounts_iter)?;

        if lottery_data_account.owner != program_id {
            msg!("Lottery Data account does not have the correct program id");
            return Err(ProgramError::IncorrectProgramId);
        }

        if !lottery_data_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut lottery_data = LotteryData::unpack_unchecked(&lottery_data_account.data.borrow())?;
        if !lottery_data.is_initialized {
            msg!("Lottery Data account is not initialized");
            return Err(LotteryError::NotInitialized.into());
        }

        lottery_data.holding_wallet = holding_wallet;
        lottery_data.rewards_wallet = rewards_wallet;
        lottery_data.slot_holders_rewards_wallet = slot_holders_rewards_wallet;

        LotteryData::pack(lottery_data, &mut lottery_data_account.data.borrow_mut())?;

        Ok(())
    }
}

// Unit tests
#[cfg(test)]
mod test {
    use super::*;
    use solana_program::{instruction::Instruction, program_pack::Pack};
    use solana_sdk::account::{
        create_account_for_test, create_is_signer_account_infos, Account as SolanaAccount,
        ReadableAccount,
    };
    use spl_token::state::Account;

    fn lottery_minimum_balance() -> u64 {
        Rent::default().minimum_balance(LotteryData::get_packed_len())
    }

    fn ticket_minimum_balance() -> u64 {
        Rent::default().minimum_balance(TicketData::get_packed_len())
    }

    fn lottery_result_minimum_balance() -> u64 {
        Rent::default().minimum_balance(LotteryResultData::get_packed_len())
    }

    fn mint_minimum_balance() -> u64 {
        Rent::default().minimum_balance(spl_token::state::Mint::LEN)
    }

    fn account_minimum_balance() -> u64 {
        Rent::default().minimum_balance(spl_token::state::Account::LEN)
    }

    fn do_process(instruction: Instruction, accounts: Vec<&mut SolanaAccount>) -> ProgramResult {
        let mut meta = instruction
            .accounts
            .iter()
            .zip(accounts)
            .map(|(account_meta, account)| (&account_meta.pubkey, account_meta.is_signer, account))
            .collect::<Vec<_>>();

        let account_infos = create_is_signer_account_infos(&mut meta);
        Processor::process(&instruction.program_id, &account_infos, &instruction.data)
    }

    #[test]
    fn test_init_lottery() {
        let program_id = id();
        let lottery_id = 112233;
        let lottery_key = Pubkey::new_unique();
        let mut lottery_acc = SolanaAccount::new(
            lottery_minimum_balance(),
            LotteryData::get_packed_len(),
            &program_id,
        );
        let mut rent_sysvar_acc = create_account_for_test(&Rent::default());
        let holding_wallet = Pubkey::new_unique();
        let rewards_wallet = Pubkey::new_unique();
        let randomness_account = Pubkey::new_unique();

        // BadCase: rent NotRentExempt
        let mut bad_lottery_acc = SolanaAccount::new(
            lottery_minimum_balance() - 100,
            LotteryData::get_packed_len(),
            &program_id,
        );
        assert_eq!(
            Err(LotteryError::NotRentExempt.into()),
            do_process(
                crate::instruction::initialize_lottery(
                    &program_id,
                    lottery_id,
                    
                    &holding_wallet,
                    &rewards_wallet,
                    
                    &randomness_account,
                    &lottery_key
                )
                .unwrap(),
                vec![&mut bad_lottery_acc, &mut rent_sysvar_acc]
            )
        );

        do_process(
            crate::instruction::initialize_lottery(
                &program_id,
                lottery_id,
               
                &holding_wallet,
                &rewards_wallet,
               
                &randomness_account,
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc, &mut rent_sysvar_acc],
        )
        .unwrap();

        // BadCase: Lottery Already initialized
        assert_eq!(
            Err(LotteryError::Initialized.into()),
            do_process(
                crate::instruction::initialize_lottery(
                    &program_id,
                    lottery_id,
                    
                    &holding_wallet,
                    &rewards_wallet,
                   
                    &randomness_account,
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc, &mut rent_sysvar_acc]
            )
        );

        let lottery = LotteryData::unpack(&lottery_acc.data).unwrap();
        assert_eq!(lottery.is_initialized, true);
        assert_eq!(lottery.lottery_id, lottery_id);
        
        assert_eq!(lottery.holding_wallet, holding_wallet);
        assert_eq!(lottery.rewards_wallet, rewards_wallet);
        
       
        assert_eq!(lottery.total_registrations, 0);
        assert_eq!(lottery.prize_pool_amount, 0);
        for number in &lottery.winning_numbers {
            assert_eq!(*number, 0);
        }
    }

    #[test]
    fn test_ticket_purchase() {
        let program_id = id();
        let lottery_id = 112233;
        let lottery_key = Pubkey::new_unique();
        let mut lottery_acc = SolanaAccount::new(
            lottery_minimum_balance(),
            LotteryData::get_packed_len(),
            &program_id,
        );
        let user_funding_key = Pubkey::new_unique();
        let mut user_funding_acc = SolanaAccount::default();
        let user_ticket_key = Pubkey::new_unique();
        let mut user_ticket_acc = SolanaAccount::new(
            ticket_minimum_balance(),
            TicketData::get_packed_len(),
            &program_id,
        );
        let mut rent_sysvar_acc = create_account_for_test(&Rent::default());
        let mut system_acc = SolanaAccount::default();
        let mut spl_token_acc = SolanaAccount::default();
        
        let holding_wallet = Pubkey::new_unique();
        let mut holding_wallet_acc = SolanaAccount::default();
        let rewards_wallet = Pubkey::new_unique();
       
        let randomness_account = Pubkey::new_unique();
        let user_charity = charity_1;

        let user_lifetime_ticket_key = Pubkey::new_unique();
        let mut user_lifetime_ticket_acc =
            SolanaAccount::new(account_minimum_balance(), Account::LEN, &spl_token::id());
        let lifetime_ticket_mint_key = Pubkey::new_unique();
        let mut lifetime_ticket_mint_acc =
            SolanaAccount::new(mint_minimum_balance(), Mint::LEN, &spl_token::id());
        let lifetime_ticket_owner_key = Pubkey::new_unique();
        let mut lifetime_ticket_owner_acc = SolanaAccount::default();

        Mint::pack(
            Mint {
                is_initialized: true,
                ..Default::default()
            },
            &mut lifetime_ticket_mint_acc.data,
        )
        .unwrap();

        // BadCase: Lottery is not initialized
        assert_eq!(
            Err(LotteryError::NotInitialized.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 50, 15],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        do_process(
            crate::instruction::initialize_lottery(
                &program_id,
                lottery_id,
               
                &holding_wallet,
                &rewards_wallet,
                
                &randomness_account,
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc, &mut rent_sysvar_acc],
        )
        .unwrap();

        // BadCase: user cannot pay
        assert_eq!(
            Err(ProgramError::InsufficientFunds),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 50, 29],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        user_funding_acc.lamports += sol_to_lamports(0.1);

        // BadCase: rent NotRentExempt
        let mut bad_ticket_acc = SolanaAccount::new(
            ticket_minimum_balance() - 100,
            TicketData::get_packed_len(),
            &program_id,
        );
        assert_eq!(
            Err(LotteryError::NotRentExempt.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 50, 15],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut bad_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        // BadCase: bad numbers
        assert_eq!(
            Err(LotteryError::InvalidNumber.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[70, 20, 30, 40, 50, 15],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        assert_eq!(
            Err(LotteryError::InvalidNumber.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 0, 15],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        assert_eq!(
            Err(LotteryError::InvalidNumber.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 50, 30],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        do_process(
            crate::instruction::purchase_ticket(
                &program_id,
                &user_charity,
                &user_funding_key,
                &[10, 20, 30, 40, 50, 29],
                &user_ticket_key,
                &holding_wallet,
                &lottery_key,
                &user_lifetime_ticket_key,
                &lifetime_ticket_owner_key,
                &lifetime_ticket_mint_key,
            )
            .unwrap(),
            vec![
                &mut lottery_acc,
                &mut user_ticket_acc,
                &mut user_funding_acc,
                &mut holding_wallet_acc,
                &mut user_lifetime_ticket_acc,
                &mut lifetime_ticket_owner_acc,
                &mut lifetime_ticket_mint_acc,
                &mut rent_sysvar_acc,
                &mut system_acc,
                &mut spl_token_acc,
            ],
        )
        .unwrap();

        let lottery = LotteryData::unpack(&lottery_acc.data()).unwrap();
        assert_eq!(lottery.charity_1_vc, 1);
        assert_eq!(lottery.total_registrations, 1);
        assert_eq!(lottery.prize_pool_amount, sol_to_lamports(0.1));

        // BadCase: Ticket already purchased
        assert_eq!(
            Err(LotteryError::AlreadyPurchased.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 50, 30],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );

        do_process(
            crate::instruction::store_winning_numbers(
                &program_id,
                &[10, 20, 30, 40, 50, 29],
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc],
        )
        .unwrap();

        let user_funding_key = Pubkey::new_unique();
        let mut user_funding_acc = SolanaAccount::default();
        let user_ticket_key = Pubkey::new_unique();
        let mut user_ticket_acc = SolanaAccount::new(
            ticket_minimum_balance(),
            TicketData::get_packed_len(),
            &program_id,
        );

        assert_eq!(
            Err(LotteryError::IsFinaled.into()),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user_charity,
                    &user_funding_key,
                    &[10, 20, 30, 40, 50, 30],
                    &user_ticket_key,
                    &holding_wallet,
                    &lottery_key,
                    &user_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user_ticket_acc,
                    &mut user_funding_acc,
                    &mut holding_wallet_acc,
                    &mut user_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ]
            )
        );
    }

    #[test]
    fn test_store_winning_numbers() {
        let program_id = id();
        let lottery_id = 112233;
        let lottery_key = Pubkey::new_unique();
        let mut lottery_acc = SolanaAccount::new(
            lottery_minimum_balance(),
            LotteryData::get_packed_len(),
            &program_id,
        );
        let mut rent_sysvar_acc = create_account_for_test(&Rent::default());
       
        let holding_wallet = Pubkey::new_unique();
        let rewards_wallet = Pubkey::new_unique();
       
        let randomness_account = Pubkey::new_unique();

        // BadCase: Lottery is not initialized
        assert_eq!(
            Err(LotteryError::NotInitialized.into()),
            do_process(
                crate::instruction::store_winning_numbers(
                    &program_id,
                    &[10, 20, 30, 40, 50, 29],
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc]
            )
        );

        do_process(
            crate::instruction::initialize_lottery(
                &program_id,
                lottery_id,
               
                &holding_wallet,
                &rewards_wallet,
               
                &randomness_account,
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc, &mut rent_sysvar_acc],
        )
        .unwrap();

        // BadCase: Bad numbers
        assert_eq!(
            Err(LotteryError::InvalidNumber.into()),
            do_process(
                crate::instruction::store_winning_numbers(
                    &program_id,
                    &[70, 20, 30, 40, 50, 29],
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc]
            )
        );

        assert_eq!(
            Err(LotteryError::InvalidNumber.into()),
            do_process(
                crate::instruction::store_winning_numbers(
                    &program_id,
                    &[10, 20, 30, 40, 0, 29],
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc]
            )
        );

        assert_eq!(
            Err(LotteryError::InvalidNumber.into()),
            do_process(
                crate::instruction::store_winning_numbers(
                    &program_id,
                    &[10, 20, 30, 40, 50, 30],
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc]
            )
        );

        do_process(
            crate::instruction::store_winning_numbers(
                &program_id,
                &[10, 20, 30, 40, 50, 29],
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc],
        )
        .unwrap();

        let lottery = LotteryData::unpack(&lottery_acc.data()).unwrap();
        assert_eq!(lottery.winning_numbers[0], 10);
        assert_eq!(lottery.winning_numbers[1], 20);
        assert_eq!(lottery.winning_numbers[2], 30);
        assert_eq!(lottery.winning_numbers[3], 40);
        assert_eq!(lottery.winning_numbers[4], 50);
        assert_eq!(lottery.winning_numbers[5], 29);
    }

    #[test]
    fn test_reward_winners() {
        let program_id = id();
        let lottery_id = 112233;
        let lottery_key = Pubkey::new_unique();
        let mut lottery_acc = SolanaAccount::new(
            lottery_minimum_balance(),
            LotteryData::get_packed_len(),
            &program_id,
        );
        let lottery_result_key = Pubkey::new_unique();
        let mut lottery_result_acc = SolanaAccount::new(
            lottery_result_minimum_balance(),
            LotteryResultData::get_packed_len(),
            &program_id,
        );
        let mut system_acc = SolanaAccount::default();
        let mut spl_token_acc = SolanaAccount::default();
        let mut rent_sysvar_acc = create_account_for_test(&Rent::default());
       
        let holding_wallet = Pubkey::new_unique();
        let mut holding_wallet_acc = SolanaAccount::default();
        let rewards_wallet = Pubkey::new_unique();
        let mut rewards_wallet_acc = SolanaAccount::default();
       

        let randomness_account = Pubkey::new_unique();

        let user1_wallet = Pubkey::new_unique();
        let mut user1_wallet_acc = SolanaAccount::default();
        let user1_ticket = Pubkey::new_unique();
        let mut user1_ticket_acc = SolanaAccount::new(
            ticket_minimum_balance(),
            TicketData::get_packed_len(),
            &program_id,
        );
        let user2_wallet = Pubkey::new_unique();
        let mut user2_wallet_acc = SolanaAccount::default();
        let user2_ticket = Pubkey::new_unique();
        let mut user2_ticket_acc = SolanaAccount::new(
            ticket_minimum_balance(),
            TicketData::get_packed_len(),
            &program_id,
        );

        let user1_lifetime_ticket_key = Pubkey::new_unique();
        let mut user1_lifetime_ticket_acc =
            SolanaAccount::new(account_minimum_balance(), Account::LEN, &spl_token::id());
        let user2_lifetime_ticket_key = Pubkey::new_unique();
        let mut user2_lifetime_ticket_acc =
            SolanaAccount::new(account_minimum_balance(), Account::LEN, &spl_token::id());
        let lifetime_ticket_mint_key = Pubkey::new_unique();
        let mut lifetime_ticket_mint_acc =
            SolanaAccount::new(mint_minimum_balance(), Mint::LEN, &spl_token::id());
        let lifetime_ticket_owner_key = Pubkey::new_unique();
        let mut lifetime_ticket_owner_acc = SolanaAccount::default();

        Mint::pack(
            Mint {
                is_initialized: true,
                ..Default::default()
            },
            &mut lifetime_ticket_mint_acc.data,
        )
        .unwrap();

        // BadCase: Lottery is not initialized
        assert_eq!(
            Err(LotteryError::NotInitialized.into()),
            do_process(
                crate::instruction::reward_winners(
                    &program_id,
                    &lottery_key,
                    &lottery_result_key,
                    &holding_wallet,
                    &rewards_wallet,
                    &vec![(user1_ticket, user1_wallet), (user2_ticket, user2_wallet)],
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut lottery_result_acc,
                    &mut holding_wallet_acc,
                    &mut rewards_wallet_acc,
                   
                    &mut system_acc,
                    &mut user1_ticket_acc,
                    &mut user1_wallet_acc,
                    &mut user2_ticket_acc,
                    &mut user2_wallet_acc
                ]
            )
        );

        do_process(
            crate::instruction::initialize_lottery(
                &program_id,
                lottery_id,
               
                &holding_wallet,
                &rewards_wallet,
                
                &randomness_account,
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc, &mut rent_sysvar_acc],
        )
        .unwrap();

        // BadCase: Lottery is not finaled
        assert_eq!(
            Err(LotteryError::NotFinaled.into()),
            do_process(
                crate::instruction::reward_winners(
                    &program_id,
                    &lottery_key,
                    &lottery_result_key,
                    &holding_wallet,
                    &rewards_wallet,
                    
                    &vec![(user1_ticket, user1_wallet), (user2_ticket, user2_wallet)],
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut lottery_result_acc,
                    &mut holding_wallet_acc,
                    &mut rewards_wallet_acc,
                    
                    &mut system_acc,
                    &mut user1_ticket_acc,
                    &mut user1_wallet_acc,
                    &mut user2_ticket_acc,
                    &mut user2_wallet_acc
                ]
            )
        );

        // BadCase: user cannot pay for ticket
        let user1_charity = charity_1;
        assert_eq!(
            Err(ProgramError::InsufficientFunds),
            do_process(
                crate::instruction::purchase_ticket(
                    &program_id,
                    &user1_charity,
                    &user1_wallet,
                    &[1, 2, 3, 4, 55, 6],
                    &user1_ticket,
                    &holding_wallet,
                    &lottery_key,
                    &user1_lifetime_ticket_key,
                    &lifetime_ticket_owner_key,
                    &lifetime_ticket_mint_key,
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut user1_ticket_acc,
                    &mut user1_wallet_acc,
                    &mut holding_wallet_acc,
                    &mut user1_lifetime_ticket_acc,
                    &mut lifetime_ticket_owner_acc,
                    &mut lifetime_ticket_mint_acc,
                    &mut rent_sysvar_acc,
                    &mut system_acc,
                    &mut spl_token_acc,
                ],
            )
        );

        // Purchase tickets
        user1_wallet_acc.lamports += sol_to_lamports(0.1);
        do_process(
            crate::instruction::purchase_ticket(
                &program_id,
                &user1_charity,
                &user1_wallet,
                &[11, 22, 33, 44, 51, 1],
                &user1_ticket,
                &holding_wallet,
                &lottery_key,
                &user1_lifetime_ticket_key,
                &lifetime_ticket_owner_key,
                &lifetime_ticket_mint_key,
            )
            .unwrap(),
            vec![
                &mut lottery_acc,
                &mut user1_ticket_acc,
                &mut user1_wallet_acc,
                &mut holding_wallet_acc,
                &mut user1_lifetime_ticket_acc,
                &mut lifetime_ticket_owner_acc,
                &mut lifetime_ticket_mint_acc,
                &mut rent_sysvar_acc,
                &mut system_acc,
                &mut spl_token_acc,
            ],
        )
        .unwrap();

        let user2_charity = charity_1;
        user2_wallet_acc.lamports += sol_to_lamports(0.1);
        do_process(
            crate::instruction::purchase_ticket(
                &program_id,
                &user2_charity,
                &user2_wallet,
                &[2, 3, 4, 5, 66, 7],
                &user2_ticket,
                &holding_wallet,
                &lottery_key,
                &user2_lifetime_ticket_key,
                &lifetime_ticket_owner_key,
                &lifetime_ticket_mint_key,
            )
            .unwrap(),
            vec![
                &mut lottery_acc,
                &mut user2_ticket_acc,
                &mut user2_wallet_acc,
                &mut holding_wallet_acc,
                &mut user2_lifetime_ticket_acc,
                &mut lifetime_ticket_owner_acc,
                &mut lifetime_ticket_mint_acc,
                &mut rent_sysvar_acc,
                &mut system_acc,
                &mut spl_token_acc,
            ],
        )
        .unwrap();

        // Store winning numbers
        do_process(
            crate::instruction::store_winning_numbers(
                &program_id,
                &[2, 3, 4, 5, 66, 7],
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc],
        )
        .unwrap();

        // BadCase: not enough users accounts
        assert_eq!(
            Err(LotteryError::InvalidParticipantsAccounts.into()),
            do_process(
                crate::instruction::reward_winners(
                    &program_id,
                    &lottery_key,
                    &lottery_result_key,
                    &holding_wallet,
                    &rewards_wallet,
                    
                    &vec![(user2_ticket, user2_wallet)],
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut lottery_result_acc,
                    &mut holding_wallet_acc,
                    &mut rewards_wallet_acc,
                    
                    &mut system_acc,
                    &mut user2_ticket_acc,
                    &mut user2_wallet_acc
                ]
            )
        );

        // BadCase: Bad wallet user pk in ticket data
        let user1_fake_wallet = Pubkey::new_unique();
        let mut user1_fake_wallet_acc = SolanaAccount::default();
        assert_eq!(
            Err(LotteryError::InvalidParticipantsAccounts.into()),
            do_process(
                crate::instruction::reward_winners(
                    &program_id,
                    &lottery_key,
                    &lottery_result_key,
                    &holding_wallet,
                    &rewards_wallet,
                   
                    &vec![
                        (user1_ticket, user1_fake_wallet),
                        (user2_ticket, user2_wallet)
                    ],
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut lottery_result_acc,
                    &mut holding_wallet_acc,
                    &mut rewards_wallet_acc,
                   
                    &mut system_acc,
                    &mut user1_ticket_acc,
                    &mut user1_fake_wallet_acc,
                    &mut user2_ticket_acc,
                    &mut user2_wallet_acc
                ]
            )
        );

        // BadCase: Bad sollotto reward account
        let fake_sollotto_labs_wallet = Pubkey::new_unique();
        assert_eq!(
            Err(LotteryError::InvalidSollottoAccount.into()),
            do_process(
                crate::instruction::reward_winners(
                    &program_id,
                    &lottery_key,
                    &lottery_result_key,
                    &holding_wallet,
                    &rewards_wallet,
                   
                    &vec![(user1_ticket, user1_wallet), (user2_ticket, user2_wallet)],
                )
                .unwrap(),
                vec![
                    &mut lottery_acc,
                    &mut lottery_result_acc,
                    &mut holding_wallet_acc,
                    &mut rewards_wallet_acc,
                   
                    &mut system_acc,
                    &mut user1_ticket_acc,
                    &mut user1_wallet_acc,
                    &mut user2_ticket_acc,
                    &mut user2_wallet_acc
                ]
            )
        );

        let lottery = LotteryData::unpack_unchecked(lottery_acc.data()).unwrap();
        assert_eq!(lottery.total_registrations, 2);
        assert_eq!(lottery.prize_pool_amount, sol_to_lamports(0.2));
        
        assert_eq!(lottery.winning_numbers, [2, 3, 4, 5, 66, 7]);

        // User2 wins the lottery
        holding_wallet_acc.lamports += sol_to_lamports(10.0);
        do_process(
            crate::instruction::reward_winners(
                &program_id,
                &lottery_key,
                &lottery_result_key,
                &holding_wallet,
                &rewards_wallet,
               
                &vec![(user1_ticket, user1_wallet), (user2_ticket, user2_wallet)],
            )
            .unwrap(),
            vec![
                &mut lottery_acc,
                &mut lottery_result_acc,
                &mut holding_wallet_acc,
                &mut rewards_wallet_acc,
               
                &mut system_acc,
                &mut user1_ticket_acc,
                &mut user1_wallet_acc,
                &mut user2_ticket_acc,
                &mut user2_wallet_acc,
            ],
        )
        .unwrap();

        // Check data
        let lottery = LotteryData::unpack_unchecked(lottery_acc.data()).unwrap();
        assert_eq!(lottery.is_initialized, false);
        assert_eq!(lottery.is_finaled, false);
        assert_eq!(lottery.total_registrations, 0);
        assert_eq!(lottery.winning_numbers, [0, 0, 0, 0, 0, 0]);
        assert_eq!(lottery.prize_pool_amount, 0);

        let lottery_result =
            LotteryResultData::unpack_unchecked(lottery_result_acc.data()).unwrap();
        assert_eq!(lottery_result.lottery_id, lottery_id);
        assert_eq!(lottery_result.winning_numbers, [2, 3, 4, 5, 66, 7]);
    }

    #[test]
    fn test_update_charity() {
        let program_id = id();
        let lottery_id = 112233;
        let lottery_key = Pubkey::new_unique();
        let mut lottery_acc = SolanaAccount::new(
            lottery_minimum_balance(),
            LotteryData::get_packed_len(),
            &program_id,
        );
        let mut rent_sysvar_acc = create_account_for_test(&Rent::default());
        let holding_wallet = Pubkey::new_unique();
        let rewards_wallet = Pubkey::new_unique();
        let randomness_account = Pubkey::new_unique();

        // BadCase: Lottery is not initialized
        assert_eq!(
            Err(LotteryError::NotInitialized.into()),
            do_process(
                crate::instruction::update_charity(
                    &program_id,
                    
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc]
            )
        );

        do_process(
            crate::instruction::initialize_lottery(
                &program_id,
                lottery_id,
                
                &holding_wallet,
                &rewards_wallet,
               
                &randomness_account,
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc, &mut rent_sysvar_acc],
        )
        .unwrap();

        
        

        
             
    }

    #[test]
    fn test_update_sollotto_wallets() {
        let program_id = id();
        let lottery_id = 112233;
        let lottery_key = Pubkey::new_unique();
        let mut lottery_acc = SolanaAccount::new(
            lottery_minimum_balance(),
            LotteryData::get_packed_len(),
            &program_id,
        );
        let mut rent_sysvar_acc = create_account_for_test(&Rent::default());
       
        let holding_wallet = Pubkey::new_unique();
        let rewards_wallet = Pubkey::new_unique();
        let randomness_account = Pubkey::new_unique();

        let new_holding_wallet = Pubkey::new_unique();
        let new_rewards_wallet = rewards_wallet;
    

        // BadCase: Lottery is not initialized
        assert_eq!(
            Err(LotteryError::NotInitialized.into()),
            do_process(
                crate::instruction::update_sollotto_wallets(
                    &program_id,
                    &new_holding_wallet,
                    &new_rewards_wallet,
                  
                    &lottery_key,
                )
                .unwrap(),
                vec![&mut lottery_acc]
            )
        );

        do_process(
            crate::instruction::initialize_lottery(
                &program_id,
                lottery_id,
               
                &holding_wallet,
                &rewards_wallet,
               
                &randomness_account,
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc, &mut rent_sysvar_acc],
        )
        .unwrap();

        
        );
        assert_eq!(lottery.sollotto_labs_wallet, sollotto_labs_wallet);

        do_process(
            crate::instruction::update_sollotto_wallets(
                &program_id,
                &new_holding_wallet,
                &new_rewards_wallet,
               
                &lottery_key,
            )
            .unwrap(),
            vec![&mut lottery_acc],
        )
        .unwrap();
        );
    }
}
