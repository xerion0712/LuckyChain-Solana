//! State transition types
use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};
use solana_program::{
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack, Sealed},
    pubkey::Pubkey,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LotteryData {
    pub is_initialized: bool,
    pub is_finaled: bool,
    pub lottery_id: u32,
   
    pub total_registrations: u32,
    pub winning_numbers: [u8; 6],
    pub prize_pool_amount: u64,
    pub holding_wallet: Pubkey,
    pub rewards_wallet: Pubkey,
    
    pub randomness_account: Pubkey,
}

impl Sealed for LotteryData {}

impl IsInitialized for LotteryData {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

//pre-built
impl Pack for LotteryData {
    /// 1 + 1 + 4 + 32 + 32 + 32 + 32 + 4 + 4 + 4 + 4 + 4 + 6 + 8 + 32 + 32 + 32 + 32 + 32 = 296
    const LEN: usize = 328;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let src = array_ref![src, 0, 328];
        let (
            is_initialized,
            is_finaled,
            lottery_id,
            
            total_registrations,
            winning_numbers,
            prize_pool_amount,
            holding_wallet,
            rewards_wallet,
            
            randomness_account,
        ) = array_refs![src, 1, 1, 4, 32, 32, 32, 32, 4, 4, 4, 4, 4, 6, 8, 32, 32, 32, 32, 32];

        let is_initialized = match is_initialized {
            [0] => false,
            [1] => true,
            _ => return Err(ProgramError::InvalidAccountData),
        };

        let is_finaled = match is_finaled {
            [0] => false,
            [1] => true,
            _ => return Err(ProgramError::InvalidAccountData),
        };

        let result = LotteryData {
            is_initialized,
            is_finaled,
            lottery_id: u32::from_le_bytes(*lottery_id),
            
            total_registrations: u32::from_le_bytes(*total_registrations),
            winning_numbers: *winning_numbers,
            prize_pool_amount: u64::from_le_bytes(*prize_pool_amount),
            holding_wallet: Pubkey::new_from_array(*holding_wallet),
            rewards_wallet: Pubkey::new_from_array(*rewards_wallet),
            
            randomness_account: Pubkey::new_from_array(*randomness_account),
        };

        Ok(result)
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 328];
        let (
            is_initialized_dst,
            is_finaled_dst,
            lottery_id_dst,
           
            total_registrations_dst,
            winning_numbers_dst,
            prize_pool_amount_dst,
            holding_wallet_dst,
            rewards_wallet_dst,
           
            randomness_account_dst,
        ) = mut_array_refs![dst, 1, 1, 4, 32, 32, 32, 32, 4, 4, 4, 4, 4, 6, 8, 32, 32, 32, 32, 32];

        is_initialized_dst[0] = self.is_initialized as u8;
        is_finaled_dst[0] = self.is_finaled as u8;
        *lottery_id_dst = self.lottery_id.to_le_bytes();
        
        *total_registrations_dst = self.total_registrations.to_le_bytes();
        *winning_numbers_dst = self.winning_numbers;
        *prize_pool_amount_dst = self.prize_pool_amount.to_le_bytes();
        holding_wallet_dst.copy_from_slice(self.holding_wallet.as_ref());
        rewards_wallet_dst.copy_from_slice(self.rewards_wallet.as_ref());
        
        randomness_account_dst.copy_from_slice(self.randomness_account.as_ref());
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TicketData {
    pub is_purchased: bool,
    pub charity: Pubkey,
    pub user_wallet_pk: Pubkey,
    pub ticket_number_arr: [u8; 6],
}

impl Sealed for TicketData {}

impl Pack for TicketData {
    /// 1 + 32 + 32 + 1 * 6 = 70
    const LEN: usize = 71;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let src = array_ref![src, 0, 71];
        let (is_purchased, charity, user_wallet_pk, ticket_number_arr) =
            array_refs![src, 1, 32, 32, 6];

        let is_purchased = match is_purchased {
            [0] => false,
            [1] => true,
            _ => return Err(ProgramError::InvalidAccountData),
        };

        let result = TicketData {
            is_purchased: is_purchased,
            charity: Pubkey::new_from_array(*charity),
            user_wallet_pk: Pubkey::new_from_array(*user_wallet_pk),
            ticket_number_arr: *ticket_number_arr,
        };

        Ok(result)
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 71];
        let (is_purchased_dst, charity_dst, user_wallet_pk_dst, ticket_number_arr_dst) =
            mut_array_refs![dst, 1, 32, 32, 6];

        is_purchased_dst[0] = self.is_purchased as u8;
        charity_dst.copy_from_slice(self.charity.as_ref());
        user_wallet_pk_dst.copy_from_slice(self.user_wallet_pk.as_ref());
        ticket_number_arr_dst.copy_from_slice(self.ticket_number_arr.as_ref());
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LotteryResultData {
    pub lottery_id: u32,
    pub winning_numbers: [u8; 6],
}

impl Sealed for LotteryResultData {}

impl Pack for LotteryResultData {
    /// 4 + 6 = 10
    const LEN: usize = 10;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let src = array_ref![src, 0, 10];
        let (lottery_id, winning_numbers) = array_refs![src, 4, 6];

        let result = LotteryResultData {
            lottery_id: u32::from_le_bytes(*lottery_id),
            winning_numbers: *winning_numbers,
        };

        Ok(result)
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 10];
        let (lottery_id_dst, winning_numbers_dst) = mut_array_refs![dst, 4, 6];

        *lottery_id_dst = self.lottery_id.to_le_bytes();
        *winning_numbers_dst = self.winning_numbers;
    }
}
