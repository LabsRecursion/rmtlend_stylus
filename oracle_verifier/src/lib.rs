
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::{vec::Vec, string::String};
use alloy_sol_types::{SolEvent, sol};
use stylus_sdk::{alloy_primitives::{U256, Address, U8, U32, U64}, prelude::*,};

sol_interface! {
    interface IRemittanceNFT {
        function mint(
            address user,
            uint256 monthly_amount,
            uint256 reliability_score,
            uint32 history_months,
            uint256 total_sent
        ) external returns (uint256);

        function update_remittance(
            uint256 token_id,
            uint256 new_monthly_amount,
            uint256 new_total_sent,
            uint256 new_reliability_score
        ) external;

        function unstake_nft(uint256 token_id) external;
    }

    interface ILoanManager {
        function process_auto_repayment(uint256 loan_id, uint256 amount) external;
        function mark_payment_missed(uint256 loan_id) external;
    }
}


sol_storage! {
    #[entrypoint]
    pub struct OracleVerifier {
        address admin;
        address remittance_nft;
        address loan_manager;
        address[] oracle_operators;
        mapping(address => VerificationRequest) verification_requests;
        mapping(uint256 => bool) monitored_loans;
    }

    pub struct VerificationRequest {
        address user;
        string provider;
        string account_id;
        uint64 request_timestamp;
        uint8 status; // 0=Pending,1=Verified,2=Failed
    }
}


sol! {
    event VerificationRequested(address indexed user);
    event VerificationComplete(address indexed user, uint256 reliability_score);
    event MonitoringStarted(uint256 indexed loan_id);
    event RemittanceReported(uint256 indexed loan_id, uint256 indexed nft_id, uint256 amount);
    event PaymentMissedReported(uint256 indexed loan_id, uint256 indexed nft_id);
}

#[public]
impl OracleVerifier {
    #[constructor]
    pub fn initialize(
        &mut self,
        remittance_nft: Address,
        loan_manager: Address,
        operators: Vec<Address>,
    ) -> Result<(), Vec<u8>> {
        if self.admin.get() != Address::ZERO {
            return Err(b"Already initialized".to_vec());
        }

        self.admin.set(self.vm().msg_sender());
        self.remittance_nft.set(remittance_nft);
        self.loan_manager.set(loan_manager);

        for op in operators {
            self.oracle_operators.push(op);
        }

        Ok(())
    }

    pub fn request_verification(&mut self, provider: String, account_id: String) -> Result<(), Vec<u8>> {
        let user = self.vm().msg_sender();
        let timestamp = U64::from(self.vm().block_timestamp());

        let mut request = self.verification_requests.setter(user);
        request.user.set(user);
        request.provider.set_str(provider);
        request.account_id.set_str(account_id);
        request.request_timestamp.set(timestamp);
        request.status.set(U8::from(0)); // Pending

        self.vm().emit_log(&VerificationRequested { user }.encode_data(), 2);
        Ok(())
    }

    pub fn submit_verification(
        &mut self,
        user: Address,
        monthly_amount: U256,
        history_months: U32,
        total_sent: U256,
        paid_count: U32,
        total_count: U32,
    ) -> Result<(), Vec<u8>> {

        if !self._is_operator(self.vm().msg_sender()) {
            return Err(b"Not authorized".to_vec());
        }

        let request = self.verification_requests.get(user);
        if request.status.get() != U8::from(0) {
            return Err(b"Already processed".to_vec());
        }

        let reliability_score = Self::_calculate_reliability_score(paid_count, total_count);
        let remittance_nft = self.remittance_nft.get();

        {
            let nft = IRemittanceNFT::new(remittance_nft);

            let _ = nft.mint(
                &mut *self,
                user,
                monthly_amount,
                U256::from(reliability_score),
                history_months.to::<u32>(),
                total_sent,
            )?;
        }

        {
            let mut request = self.verification_requests.setter(user);
            request.status.set(U8::from(1)); // Verified
        }

        self.vm().emit_log(
            &VerificationComplete { 
                user, 
                reliability_score: U256::from(reliability_score) 
            }
            .encode_data(), 2);

        Ok(())
    }

    pub fn start_monitoring_loan(&mut self, loan_id: U256) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.loan_manager.get() {
            return Err(b"Only loan manager".to_vec());
        }

        self.monitored_loans.insert(loan_id, true);
        self.vm().emit_log(&MonitoringStarted { loan_id }.encode_data(), 2);
        Ok(())
    }

    pub fn report_remittance(
        &mut self,
        user: Address,
        nft_id: U256,
        amount: U256,
        loan_id: U256,
    ) -> Result<(), Vec<u8>> {
        if !self._is_operator(self.vm().msg_sender()) {
            return Err(b"Not authorized".to_vec());
        }

        if !self.monitored_loans.get(loan_id) {
            return Err(b"Loan not monitored".to_vec());
        }

        {
            let nft = IRemittanceNFT::new(self.remittance_nft.get());
            nft.update_remittance(&mut *self, nft_id, amount, amount, U256::from(90u64))?;
        }

        {
            let loan_mgr = ILoanManager::new(self.loan_manager.get());
            loan_mgr.process_auto_repayment(&mut *self, loan_id, amount)?;
        }

        self.vm().emit_log(&RemittanceReported { loan_id, nft_id, amount }.encode_data(), 3);
        Ok(())
    }

    pub fn report_missed_payment(&mut self, loan_id: U256, nft_id: U256) -> Result<(), Vec<u8>> {
        if !self._is_operator(self.vm().msg_sender()) {
            return Err(b"Not authorized".to_vec());
        }

        {
            let nft = IRemittanceNFT::new(self.remittance_nft.get());
            nft.unstake_nft(&mut *self, nft_id)?;
        }

        {
            let loan_mgr = ILoanManager::new(self.loan_manager.get());
            loan_mgr.mark_payment_missed(&mut *self, loan_id)?;
        }

        self.vm().emit_log(&PaymentMissedReported { loan_id, nft_id }.encode_data(), 2);
        Ok(())
    }

    pub fn get_verification_status(&self, user: Address) -> U8 {
        self.verification_requests.get(user).status.get()
    }

    // --- Internal helpers ---

    fn _is_operator(&self, op: Address) -> bool {
        for i in 0..self.oracle_operators.len() {
            if self.oracle_operators.get(i).unwrap() == op {
                return true;
            }
        }
        false
    }

    fn _calculate_reliability_score(paid: U32, total: U32) -> u32 {
        if total == U32::from(0u64) {
            100
        } else {
            ((paid * U32::from(100u64)) / total).to::<u32>()
        }
    }
}
