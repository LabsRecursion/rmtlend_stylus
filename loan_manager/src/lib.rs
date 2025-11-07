
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_sol_types::{SolEvent, sol};
use stylus_sdk::{alloy_primitives::{U256, Address, U8, U32, U64}, prelude::*, storage::{StorageVec, StorageU256}};

sol_storage! {
    #[entrypoint]
    pub struct LoanManager {
        address admin;
        address oracle;
        address remittance_nft;
        address lending_pool;
        address usdc;
        uint256 loan_counter;
        mapping(uint256 => Loan) loans;
        mapping(address => StorageVec<StorageU256>) borrower_loans;
    }

    pub struct Loan {
        uint256 loan_id;
        address borrower;
        uint256 nft_collateral_id;
        uint256 loan_amount;
        uint256 outstanding_balance;
        uint256 total_repaid;
        uint32 interest_rate_bps;
        uint32 duration_months;
        uint256 monthly_payment;
        uint64 start_timestamp;
        uint64 next_payment_due;
        uint8 status;             // 0=Pending,1=Active,2=Repaid,3=Defaulted
        uint32 payments_made;
        uint32 payments_missed;
    }
}

sol_interface! {
    interface IERC20 {
        function transferFrom(address from, address to, uint256 value) external returns (bool);
    }

    interface IRemittanceNFT {
        function getRemittance(uint256 token_id)
            external
            view
            returns (address, uint256, uint256, uint256, bool);
        function stakeNFT(uint256 token_id, uint256 loan_id) external;
        function unstakeNFT(uint256 token_id) external;
    }

    interface ILendingPool {
        function borrow(uint256 amount, address borrower, uint256 loan_id) external;
        function repay(uint256 principal, uint256 interest, uint256 loan_id) external;
    }
}

sol! {
    event LoanRequested(address indexed borrower, uint256 indexed loan_id);
    event LoanApproved(uint256 indexed loan_id);
    event PaymentMade(uint256 indexed loan_id, uint256 amount);
    event PaymentMissed(uint256 indexed loan_id, uint32 missed_count);
}

#[public]
impl LoanManager {

    #[constructor]
    pub fn initialize(
        &mut self,
        remittance_nft: Address,
        lending_pool: Address,
        oracle: Address,
        usdc: Address,
    ) -> Result<(), Vec<u8>> {
        if self.admin.get() != Address::ZERO {
            return Err(b"Already initialized".to_vec());
        }
        self.admin.set(self.vm().msg_sender());
        self.remittance_nft.set(remittance_nft);
        self.lending_pool.set(lending_pool);
        self.oracle.set(oracle);
        self.usdc.set(usdc);
        self.loan_counter.set(U256::ZERO);
        Ok(())
    }

    pub fn request_loan(
        &mut self,
        nft_id: U256,
        amount: U256,
        duration_months: u32,
    ) -> Result<U256, Vec<u8>> {
        let borrower = self.vm().msg_sender();

        // let (owner, _, reliability_score, _, _) = IRemittanceNFT::new(self.remittance_nft.get())
            // .getRemittance(nft_id);
        let remittance_nft = IRemittanceNFT::new(self.remittance_nft.get());
        let (owner, _, reliability_score, _, _) = remittance_nft.get_remittance(&mut *self, nft_id)?;

        if owner != borrower {
            return Err(b"NFT does not belong to borrower".to_vec());
        }

        let interest_rate_bps = Self::_calculate_interest_rate(reliability_score);
        let monthly_payment =
            Self::_calculate_monthly_payment(amount, interest_rate_bps, duration_months);
        let current_time = U64::from(self.vm().block_timestamp());
        let next_pay_date = U64::from(self.vm().block_timestamp().saturating_add( 30 * 24 * 60 * 60));

        let loan_id = self.loan_counter.get() + U256::from(1u64);
        self.loan_counter.set(loan_id);

        let mut loan = self.loans.setter(loan_id);
        loan.loan_id.set(loan_id);
        loan.borrower.set(borrower);
        loan.nft_collateral_id.set(nft_id);
        loan.loan_amount.set(amount);
        loan.outstanding_balance.set(amount);
        loan.total_repaid.set(U256::ZERO);
        loan.interest_rate_bps.set(U32::from(interest_rate_bps));
        loan.duration_months.set(U32::from(duration_months));
        loan.monthly_payment.set(monthly_payment);
        loan.start_timestamp.set(current_time);
        loan.next_payment_due.set(next_pay_date);
        loan.status.set(U8::from(0));
        loan.payments_made.set(U32::from(0));
        loan.payments_missed.set(U32::from(0));


        // self.loans.insert(loan_id, loan);

        // let mut list = self.borrower_loans.get(borrower);
        // list.push(loan_id);
        // self.borrower_loans.insert(borrower, list);

        let req_loan = LoanRequested { borrower, loan_id };
        self.vm().emit_log(&req_loan.encode_data(), 2);
        Ok(loan_id)
    }

    pub fn approve_loan(&mut self, loan_id: U256) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.admin.get() {
            return Err(b"Only admin".to_vec());
        }

        let loan = self.loans.getter(loan_id);
        let  loan_amount = loan.loan_amount.get();
        let borrower = loan.borrower.get();
        let nft_id = loan.nft_collateral_id.get();
        if loan.status.get() != U8::from(0) {
            return Err(b"Loan not pending".to_vec());
        }

        {
            let _ = IRemittanceNFT::new(self.remittance_nft.get())
                .stake_nft(
                    &mut *self, 
                    nft_id, 
                    loan_id
                )?;

            let _ = ILendingPool::new(self.lending_pool.get())
                .borrow(
                    &mut *self, 
                    loan_amount,
                    borrower,
                    loan_id,    
                )?;
        }

        {
            let mut loan = self.loans.setter(loan_id);
            loan.status.set(U8::from(1));
        }

        let approve_loan = LoanApproved { loan_id };
        self.vm().emit_log(&approve_loan.encode_data(), 1);
        Ok(())
    }

    pub fn make_payment(&mut self, loan_id: U256, amount: U256) -> Result<(), Vec<u8>> {
        let sender = self.vm().msg_sender();
        let loan = self.loans.getter(loan_id);
        if loan.status.get() != U8::from(1) {
            return Err(b"Loan not active".to_vec());
        }
        if sender != loan.borrower.get() {
            return Err(b"Only borrower can pay".to_vec());
        }
        self._process_payment(loan_id, sender, amount)
    }

    pub fn process_auto_repayment(
        &mut self,
        loan_id: U256,
        remittance_amount: U256,
    ) -> Result<U256, Vec<u8>> {
        if self.vm().msg_sender() != self.oracle.get() {
            return Err(b"Only oracle".to_vec());
        }
        let loan = self.loans.getter(loan_id);
        let payment_amount = if remittance_amount >= loan.monthly_payment.get() {
            loan.monthly_payment.get()
        } else {
            remittance_amount
        };
        self._process_payment(loan_id, loan.borrower.get(), payment_amount)?;
        Ok(remittance_amount - payment_amount)
    }

    // ---- Mark payment missed ----
    pub fn mark_payment_missed(&mut self, loan_id: U256) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.oracle.get() {
            return Err(b"Only oracle".to_vec());
        }

        let mut loan = self.loans.setter(loan_id);
        let missed = loan.payments_missed.get().saturating_add(U32::from(1));
        loan.payments_missed.set(missed);

        if missed >= U32::from(2u64) {
            loan.status.set(U8::from(3)); // Defaulted
        }

        Ok(())
    }

    fn _process_payment(
        &mut self,
        loan_id: U256,
        payer: Address,
        amount: U256,
    ) -> Result<(), Vec<u8>> {
        if amount == U256::ZERO {
            return Err(b"Amount must be > 0".to_vec());
        }

        let lending_pool = self.lending_pool.get();
        let remittance_nft_addr = self.remittance_nft.get();
        let usdc = self.usdc.get();
        let loan = self.loans.getter(loan_id);
        let outstanding = loan.outstanding_balance.get();
        let interest_rate_bps = loan.interest_rate_bps.get();
        let nft_id = loan.nft_collateral_id.get();
        let interest_portion = Self::_calculate_interest_portion(outstanding, interest_rate_bps);
        let total_repaid = loan.total_repaid.get();
        let payments_made = loan.payments_made.get();
        let next_payment_due = loan.next_payment_due.get();
        // let payments_missed = loan.payments_missed.get();
        let status = loan.status.get();

        if status != U8::from(1) {
            return Err(b"Loan not active".to_vec());
        }

        let mut principal_portion = if amount > interest_portion {
            amount - interest_portion
        } else {
            U256::ZERO
        };

        // ERC20 Transfer
        {
            let erc20 = IERC20::new(usdc);
            erc20.transfer_from(&mut *self, payer, lending_pool, amount)?;
        }

        {
            let pool = ILendingPool::new(lending_pool);
            pool.repay(&mut *self, principal_portion, interest_portion, loan_id)?;
        }

        if principal_portion >= outstanding {
            principal_portion = outstanding;

            let nft: IRemittanceNFT = IRemittanceNFT::new(remittance_nft_addr);
            let _ = nft.unstake_nft(&mut *self, nft_id)?;
        }
        else {}

        {
            let mut loan = self.loans.setter(loan_id);
            loan.total_repaid
                .set(total_repaid);
            loan.payments_made
                .set(payments_made);
            loan.next_payment_due
                .set(next_payment_due);

            if principal_portion >= outstanding {
                loan.outstanding_balance.set(U256::ZERO);
                loan.status.set(U8::from(2)); // 2 = Fully repaid or closed
            }
            else {
                loan.outstanding_balance.set(outstanding - principal_portion);
            }
        }

        // Emit event
        let event = PaymentMade { loan_id, amount };
        self.vm().emit_log(&event.encode_data(), 2);

        Ok(())
    }

    fn _calculate_interest_rate(score: U256) -> u32 {
        let s = (score % U256::from(100u64)).to::<u64>();
        if s >= 90 {
            1500
        } else if s >= 80 {
            2000
        } else if s >= 70 {
            3000
        } else {
            4000
        }
    }

    fn _calculate_monthly_payment(principal: U256, rate_bps: u32, months: u32) -> U256 {
        let total_interest = principal * U256::from(rate_bps) * U256::from(months)
            / U256::from(12u64 * 10000u64);
        let total = principal + total_interest;
        if months == 0 {
            total
        } else {
            total / U256::from(months)
        }
    }

    fn _calculate_interest_portion(outstanding: U256, annual_rate_bps: U32) -> U256 {
        let monthly_rate = annual_rate_bps / U32::from(12u64);
        (outstanding * U256::from(monthly_rate)) / U256::from(10000u64)
    }
}
