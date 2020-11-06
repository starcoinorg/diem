// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{ensure, Error, Result};
use diem_crypto::{
    hash::{CryptoHash, CryptoHasher},
    x25519, HashValue,
};
use diem_crypto_derive::CryptoHasher;
#[cfg(any(test, feature = "fuzzing"))]
use proptest_derive::Arbitrary;
use rand::{rngs::OsRng, Rng};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{convert::TryFrom, fmt, str::FromStr};

/// A struct that represents an account address.
#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Copy, CryptoHasher)]
#[cfg_attr(any(test, feature = "fuzzing"), derive(Arbitrary))]
pub struct AccountAddress([u8; AccountAddress::LENGTH]);

impl AccountAddress {
    pub const fn new(address: [u8; Self::LENGTH]) -> Self {
        Self(address)
    }

    /// The number of bytes in an address.
    pub const LENGTH: usize = 16;

    /// Hex address: 0x0
    pub const ZERO: Self = Self([0u8; Self::LENGTH]);

    pub fn random() -> Self {
        let mut rng = OsRng;
        let buf: [u8; Self::LENGTH] = rng.gen();
        Self(buf)
    }

    pub fn short_str_lossless(&self) -> String {
        let hex_str = hex::encode(&self.0).trim_start_matches('0').to_string();
        if hex_str.is_empty() {
            "0".to_string()
        } else {
            hex_str
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn to_u8(self) -> [u8; Self::LENGTH] {
        self.0
    }

    pub fn from_hex_literal(literal: &str) -> Result<Self> {
        let literal = literal.strip_prefix("0x").unwrap_or_else(|| literal);
        let hex_len = literal.len();
        let mut result = if hex_len % 2 != 0 {
            let mut hex_str = String::with_capacity(hex_len + 1);
            hex_str.push('0');
            hex_str.push_str(literal);
            hex::decode(&hex_str)?
        } else {
            hex::decode(literal)?
        };

        let len = result.len();
        let padded_result = if len < Self::LENGTH {
            let mut padded = Vec::with_capacity(Self::LENGTH);
            padded.resize(Self::LENGTH - len, 0u8);
            padded.append(&mut result);
            padded
        } else {
            result
        };

        AccountAddress::try_from(padded_result)
    }

    // Note: This is inconsistent with current types because AccountAddress is derived
    // from consensus key which is of type Ed25519PublicKey. Since AccountAddress does
    // not mean anything in a setting without remote authentication, we use the network
    // public key to generate a peer_id for the peer.
    // See this issue for potential improvements: https://github.com/diem/diem/issues/3960
    pub fn from_identity_public_key(identity_public_key: x25519::PublicKey) -> Self {
        let mut array = [0u8; Self::LENGTH];
        let pubkey_slice = identity_public_key.as_slice();
        // keep only the last 16 bytes
        array.copy_from_slice(&pubkey_slice[x25519::PUBLIC_KEY_SIZE - Self::LENGTH..]);
        Self(array)
    }
}

impl CryptoHash for AccountAddress {
    type Hasher = AccountAddressHasher;

    fn hash(&self) -> HashValue {
        let mut state = Self::Hasher::default();
        state.update(&self.0);
        state.finish()
    }
}

impl AsRef<[u8]> for AccountAddress {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::ops::Deref for AccountAddress {
    type Target = [u8; Self::LENGTH];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for AccountAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        // Forward to the LowerHex impl with a "0x" prepended (the # flag).
        write!(f, "0x{:#x}", self)
    }
}

impl fmt::Debug for AccountAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Forward to the LowerHex impl with a "0x" prepended (the # flag).
        write!(f, "0x{:#x}", self)
    }
}

impl fmt::LowerHex for AccountAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

impl fmt::UpperHex for AccountAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode_upper(&self.0))
    }
}

impl TryFrom<&[u8]> for AccountAddress {
    type Error = Error;

    /// Tries to convert the provided byte array into Address.
    fn try_from(bytes: &[u8]) -> Result<AccountAddress> {
        ensure!(
            bytes.len() == Self::LENGTH,
            "The Address {:?} is of invalid length",
            bytes
        );
        let mut addr = [0u8; Self::LENGTH];
        addr.copy_from_slice(bytes);
        Ok(AccountAddress(addr))
    }
}

impl TryFrom<&[u8; AccountAddress::LENGTH]> for AccountAddress {
    type Error = Error;

    /// Tries to convert the provided byte array into Address.
    fn try_from(bytes: &[u8; Self::LENGTH]) -> Result<AccountAddress> {
        AccountAddress::try_from(&bytes[..])
    }
}

impl TryFrom<Vec<u8>> for AccountAddress {
    type Error = Error;

    /// Tries to convert the provided byte buffer into Address.
    fn try_from(bytes: Vec<u8>) -> Result<AccountAddress> {
        AccountAddress::try_from(&bytes[..])
    }
}

impl From<AccountAddress> for Vec<u8> {
    fn from(addr: AccountAddress) -> Vec<u8> {
        addr.0.to_vec()
    }
}

impl From<&AccountAddress> for Vec<u8> {
    fn from(addr: &AccountAddress) -> Vec<u8> {
        addr.0.to_vec()
    }
}

impl From<AccountAddress> for [u8; AccountAddress::LENGTH] {
    fn from(addr: AccountAddress) -> Self {
        addr.0
    }
}

impl From<&AccountAddress> for [u8; AccountAddress::LENGTH] {
    fn from(addr: &AccountAddress) -> Self {
        addr.0
    }
}

impl From<&AccountAddress> for String {
    fn from(addr: &AccountAddress) -> String {
        ::hex::encode(addr.as_ref())
    }
}

impl TryFrom<String> for AccountAddress {
    type Error = Error;

    fn try_from(s: String) -> Result<AccountAddress> {
        AccountAddress::from_str(s.as_str())
    }
}

impl FromStr for AccountAddress {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        AccountAddress::from_hex_literal(s)
    }
}

impl<'de> Deserialize<'de> for AccountAddress {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = <String>::deserialize(deserializer)?;
            AccountAddress::from_str(&s).map_err(D::Error::custom)
        } else {
            // In order to preserve the Serde data model and help analysis tools,
            // make sure to wrap our value in a container with the same name
            // as the original type.
            #[derive(::serde::Deserialize)]
            #[serde(rename = "AccountAddress")]
            struct Value([u8; AccountAddress::LENGTH]);

            let value = Value::deserialize(deserializer)?;
            Ok(AccountAddress::new(value.0))
        }
    }
}

impl Serialize for AccountAddress {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            self.to_string().serialize(serializer)
        } else {
            // See comment in deserialize.
            serializer.serialize_newtype_struct("AccountAddress", &self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::account_address::AccountAddress;
    use super::*;
    use hex::FromHex;

    #[test]
    fn test_serialize() {
        let addr = AccountAddress::random();

        let json_value = serde_json::to_string(&addr).unwrap();
        println!("{}", json_value);
        assert_eq!(json_value, format!("\"{}\"", addr.to_string()));
        let de_addr = serde_json::from_slice::<AccountAddress>(json_value.as_bytes()).unwrap();
        assert_eq!(addr, de_addr);
    }
    #[test]
    fn test_short_str_lossless() {
        let hex = Vec::from_hex("00c0f1f95c5b1c5f0eda533eff269000")
            .expect("You must provide a valid Hex format");

        let address: AccountAddress = AccountAddress::try_from(&hex[..]).unwrap_or_else(|_| {
            panic!(
                "The address {:?} is of invalid length. Addresses must be 16-bytes long",
                &hex
            )
        });

        let string_lossless = address.short_str_lossless();

        assert_eq!(
            "c0f1f95c5b1c5f0eda533eff269000".to_string(),
            string_lossless
        );
    }

    #[test]
    fn test_short_str_lossless_zero() {
        let hex = Vec::from_hex("00000000000000000000000000000000")
            .expect("You must provide a valid Hex format");

        let address: AccountAddress = AccountAddress::try_from(&hex[..]).unwrap_or_else(|_| {
            panic!(
                "The address {:?} is of invalid length. Addresses must be 16-bytes long",
                &hex
            )
        });

        let string_lossless = address.short_str_lossless();

        assert_eq!("0".to_string(), string_lossless);
    }
}
