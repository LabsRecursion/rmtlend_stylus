
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use stylus_sdk::{
    alloy_primitives::{
        U256, Address, 
        U32, U64
    }, prelude::*
};

sol_interface! {
    interface IERC20 {
        function transferFrom(address from, address to, uint256 tokens) external;
        function transfer(address to, uint256 tokens) external;
        function balanceOf(address owner) external view returns (uint256);
    }
}

sol_storage! {
    #[entrypoint]
    pub struct LendingPool {
        address usdc_token;
        address loan_manager;
        uint32 base_interest_rate;
        uint32 max_utilization;

        uint256 total_liquidity;
        uint256 total_borrowed;
        uint256 total_interest_earned;
        uint256 accumulated_interest_per_share;

        mapping(address => LenderInfo) lenders;
    }

    pub struct LenderInfo {
        uint256 deposit_amount;
        uint64 deposit_timestamp;
        uint256 earned_interest;
        uint32 share_percentage;
        uint256 last_acc_interest_per_share;
    }
}

#[public]
impl LendingPool {

    #[constructor]
    pub fn initialize(&mut self, loan_manager: Address, usdc_token: Address, base_rate: u32) -> Result<(), Vec<u8>> {
        if self.loan_manager.get() != Address::ZERO {
            return Err(b"Already initialized".to_vec());
        }
        self.loan_manager.set(loan_manager);
        self.usdc_token.set(usdc_token);
        self.base_interest_rate.set(U32::from(base_rate));
        self.max_utilization.set(U32::from(9000)); // 90%
        Ok(())
    }

    pub fn deposit(&mut self, amount: U256) -> Result<(), Vec<u8>> {
        let sender: Address = self.vm().msg_sender();
        if amount == U256::ZERO {
            return Err(b"Invalid amount".to_vec());
        }

        // toks from lender
        {
            let token = self.usdc_token.get();
            let contract = self.vm().contract_address();
            let erc20 = IERC20::new(token);
            let _ = erc20.transfer_from(&mut *self, sender, contract, amount);
        }

        let pending;
        {
            pending = self.update_interest(sender);
        }

        // lender info
        let lender = self.lenders.get(sender);
        let new_deposit = lender.deposit_amount.get().saturating_add(amount);
        let current_time =  self.vm().block_timestamp();
        

        // pending interest
        let mut accrued_interest = U256::ZERO;
        {
            if lender.deposit_amount.get() == U256::ZERO {
            
                if pending > U256::ZERO {
                    accrued_interest = pending;
                }
            }
        }

        // set total liquidity
        let new_total_liq = self.total_liquidity.get().saturating_add(amount);
        self.total_liquidity.set(new_total_liq);

        // set share percentage
        let new_share = if new_total_liq > U256::ZERO {
            (new_deposit * U256::from(10000)) / new_total_liq
        } else {
            U256::from(10000)
        };

        {
            // set deposit values
            let mut lender = self.lenders.setter(sender);
            lender.earned_interest.set(accrued_interest);
            lender.share_percentage.set(U32::from(new_share));
            lender.deposit_amount.set(new_deposit);
            lender.deposit_timestamp.set(U64::from(current_time));
        }

        Ok(())
    }

    pub fn withdraw(&mut self, amount: U256) -> Result<(), Vec<u8>> {
        let sender = self.vm().msg_sender();
        
        // Validate amount
        if amount == U256::ZERO {
            return Err(b"Invalid amount".to_vec());
        }

        // Get lender info
        let lender = self.lenders.getter(sender);
        let deposit_amount = lender.deposit_amount.get();
        
        // Check sufficient balance
        if deposit_amount < amount {
            return Err(b"Insufficient balance".to_vec());
        }

        // Check pool liquidity
        let total_liq = self.total_liquidity.get();
        let available = total_liq.saturating_sub(self.total_borrowed.get());
        
        if amount > available {
            return Err(b"Insufficient pool liquidity".to_vec());
        }

        // Claim pending interest
        let pending = self.update_interest(sender);
        
        // Calculate new deposit amount
        let new_deposit = deposit_amount.saturating_sub(amount);
        
        // Update total liquidity
        let new_total_liq = total_liq.saturating_sub(amount);
        self.total_liquidity.set(U256::from(new_total_liq));

        // Update lender's state
        {
            let mut lender = self.lenders.setter(sender);
            lender.deposit_amount.set(new_deposit);
            
            // Update share percentage
            let new_share = if new_total_liq > U256::ZERO {
                (new_deposit * U256::from(10000)) / new_total_liq
            } else {
                U256::ZERO
            };
            lender.share_percentage.set(U32::from(new_share));
        }

        // Transfer tokens to sender
        let total_withdraw = amount.saturating_add(pending);
        let token = IERC20::new(self.usdc_token.get());
        
        let _ = token.transfer(&mut *self, sender, total_withdraw);

        Ok(())
    }

    pub fn borrow(&mut self, amount: U256, borrower: Address) {
        let caller = self.vm().msg_sender();
        assert!(caller == self.loan_manager.get(), "Not LoanManager");
        assert!(amount > U256::ZERO, "Invalid amount");

        let total_liq = self.total_liquidity.get();
        let total_borrowed = self.total_borrowed.get();
        assert!(total_liq >= total_borrowed + amount, "Insufficient liquidity");

        self.total_borrowed.set(total_borrowed + amount);

        let token = IERC20::new(self.usdc_token.get());
        let _ = token.transfer(&mut *self, borrower, amount);
        // assert!(success, "Borrow transfer failed");

    }

    pub fn repay(&mut self, principal: U256, interest: U256) {
        let caller = self.vm().msg_sender();
        assert!(caller == self.loan_manager.get(), "Not LoanManager");

        let mut total_borrowed = self.total_borrowed.get();
        let mut total_interest_earned = self.total_interest_earned.get();

        total_borrowed -= principal;
        total_interest_earned += interest;

        self.total_borrowed.set(total_borrowed);
        self.total_interest_earned.set(total_interest_earned);

        // Update accumulated interest per share
        let total_liq = self.total_liquidity.get();
        if interest > U256::ZERO && total_liq > U256::ZERO {
            let interest_per_share = (interest * U256::from(1_000_000_000u64)) / total_liq;
            let mut acc = self.accumulated_interest_per_share.get();
            acc += interest_per_share;
            self.accumulated_interest_per_share.set(acc);
        }
    }

    pub fn get_available_liquidity(&self) -> U256 {
        self.total_liquidity.get() - self.total_borrowed.get()
    }

    pub fn get_utilization_rate(&self) -> U256 {
        let total_liq = self.total_liquidity.get();
        if total_liq == U256::ZERO {
            return U256::ZERO;
        }
        let total_borrowed = self.total_borrowed.get();
        (total_borrowed * U256::from(10000)) / total_liq
    }

    pub fn get_lender_info(&self, lender: Address) -> (U256, U256, U32, U256) {
        let lender = self.lenders.getter(lender);
        (
            lender.deposit_amount.get(), 
            lender.earned_interest.get(), 
            lender.share_percentage.get(), 
            lender.last_acc_interest_per_share.get()
        )
    }

    fn update_interest(&mut self, lender_addr: Address) -> U256 {
        let lender = self.lenders.getter(lender_addr);

        let acc = self.accumulated_interest_per_share.get();
        let last_acc = lender.last_acc_interest_per_share.get();

        let mut pending = U256::ZERO;

        if lender.deposit_amount.get() > U256::ZERO {
            pending = (lender.deposit_amount.get() * (acc.clone() - last_acc)) / U256::from(1_000_000_000u64);
            pending = lender.earned_interest.get() + pending;
        }

        // lender.last_acc_interest_per_share.set(acc);
        let mut _kk = self.lenders.setter(lender_addr);
        _kk.last_acc_interest_per_share.set(acc);
        _kk.earned_interest.set(pending);

        pending
    }
}
