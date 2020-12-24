// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::account_address::AccountAddress;
use crate::parser::parse_transaction_argument;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum TransactionArgument {
    U8(u8),
    U64(u64),
    U128(u128),
    Address(AccountAddress),
    U8Vector(#[serde(with = "serde_bytes")] Vec<u8>),
    Bool(bool),
}

impl fmt::Debug for TransactionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionArgument::U8(value) => write!(f, "{{U8: {}}}", value),
            TransactionArgument::U64(value) => write!(f, "{{U64: {}}}", value),
            TransactionArgument::U128(value) => write!(f, "{{U128: {}}}", value),
            TransactionArgument::Bool(boolean) => write!(f, "{{BOOL: {}}}", boolean),
            TransactionArgument::Address(address) => write!(f, "{{ADDRESS: {:?}}}", address),
            TransactionArgument::U8Vector(vector) => {
                write!(f, "{{U8Vector: 0x{}}}", hex::encode(vector))
            }
        }
    }
}

/// impl display for transaction argument.
/// It is a reverse of parser.parse_transaction_argument.
impl fmt::Display for TransactionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionArgument::U8(value) => write!(f, "{}u8", value),
            TransactionArgument::U64(value) => write!(f, "{}u64", value),
            TransactionArgument::U128(value) => write!(f, "{}u128", value),
            TransactionArgument::Bool(boolean) => write!(f, "{}", boolean),
            TransactionArgument::Address(address) => write!(f, "{}", address),
            TransactionArgument::U8Vector(vector) => write!(f, "x\"{}\"", hex::encode(vector)),
        }
    }
}

#[test]
fn test_transaction_argument_display() {
    for arg in &[
        TransactionArgument::U128(1),
        TransactionArgument::U64(1),
        TransactionArgument::U8(1),
        TransactionArgument::Bool(true),
        TransactionArgument::Address(AccountAddress::random()),
        TransactionArgument::U8Vector(vec![0xde, 0xad, 0xbe, 0xef]),
    ] {
        println!("{}", arg);
        let actual = parse_transaction_argument(&arg.to_string()).unwrap();

        assert_eq!(arg, &actual);
    }
}
