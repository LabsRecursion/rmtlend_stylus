
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use stylus_sdk::{
    alloy_primitives::{Address, U256},
    prelude::*,
    storage::{StorageBool, StorageMap, StorageString, StorageU256},
};

#[storage]
#[entrypoint]
pub struct AutoMintErc20 {
    pub name: StorageString,
    pub symbol: StorageString,
    pub decimals: StorageU256,
    pub total_supply: StorageU256,
    pub balances: StorageMap<Address, StorageU256>,
    pub allowances: StorageMap<Address, StorageMap<Address, StorageU256>>,
    pub minted: StorageMap<Address, StorageBool>,
}

const AIRDROP_AMOUNT: u64 = 5_000_000_000_000_000_000;

#[public]
impl AutoMintErc20 {
    pub fn name(&self) -> String {
        self.name.get_string()
    }

    pub fn symbol(&self) -> String {
        self.symbol.get_string()
    }

    pub fn decimals(&self) -> U256 {
        self.decimals.get()
    }

    pub fn total_supply(&self) -> U256 {
        self.total_supply.get()
    }

    pub fn balance_of(&mut self, owner: Address) -> U256 {
        self.ensure_airdrop(owner);
        self.balances.get(owner)
    }

    pub fn allowance(&mut self, owner: Address, spender: Address) -> U256 {
        self.ensure_airdrop(owner);
        self.allowances.get(owner).get(spender)
    }

    pub fn transfer(&mut self, to: Address, amount: U256) -> bool {
        let sender = self.vm().msg_sender();
        self.ensure_airdrop(sender);
        self._transfer(sender, to, amount)
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> bool {
        let sender = self.vm().msg_sender();
        self.ensure_airdrop(sender);
        self.allowances.setter(sender).setter(spender).set(amount);
        true
    }

    pub fn transfer_from(&mut self, from: Address, to: Address, amount: U256) -> bool {
        self.ensure_airdrop(from);
        let sender = self.vm().msg_sender();
        let current_allowance = self.allowances.get(from).get(sender);
        assert!(current_allowance >= amount, "ERC20: insufficient allowance");

        self.allowances
            .setter(from)
            .setter(sender)
            .set(current_allowance - amount);
        self._transfer(from, to, amount)
    }
}

impl AutoMintErc20 {
    fn _transfer(&mut self, from: Address, to: Address, amount: U256) -> bool {
        assert!(amount > U256::from(0), "ERC20: zero transfer");

        let from_balance = self.balances.get(from);
        assert!(from_balance >= amount, "ERC20: insufficient balance");

        self.balances.setter(from).set(from_balance - amount);

        let to_balance = self.balances.get(to);
        self.balances.setter(to).set(to_balance + amount);

        true
    }

    fn ensure_airdrop(&mut self, user: Address) {
        if !self.minted.get(user) {
            let amt = U256::from(AIRDROP_AMOUNT);
            let new_bal = self.balances.get(user) + amt;
            self.balances.setter(user).set(new_bal);
            self.total_supply.set(self.total_supply.get() + amt);
            self.minted.setter(user).set(true);
        }
    }
}