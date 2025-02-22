use std::collections::HashMap;
use std::fs;
use std::io::Error as IoError;
use std::path::Path;

use eyre::Result;
use kakarot_rpc_core::client::constants::STARKNET_NATIVE_TOKEN;
use kakarot_rpc_core::test_utils::deploy_helpers::compute_kakarot_contracts_class_hash;
use lazy_static::lazy_static;
use pallet_starknet::genesis_loader::{ContractClass, GenesisLoader, HexFelt};
use reth_primitives::{Address, Bytes, H256, U256, U64};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;

use crate::kakarot::compute_starknet_address;
use crate::madara::utils::{
    genesis_fund_starknet_address, genesis_set_bytecode, genesis_set_storage_kakarot_contract_account,
    genesis_set_storage_starknet_contract,
};
use crate::types::Felt;

/// Types from https://github.com/ethereum/go-ethereum/blob/master/core/genesis.go#L49C1-L58
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveGenesisConfig {
    pub config: Config,
    pub coinbase: Address,
    pub difficulty: U64,
    pub extra_data: Bytes,
    pub gas_limit: U64,
    pub nonce: U64,
    pub timestamp: U64,
    pub alloc: HashMap<Address, AccountInfo>,
}

impl HiveGenesisConfig {
    pub fn from_file(path: &str) -> Result<Self> {
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }
}

// Define constant addresses for Kakarot contracts
lazy_static! {
    pub static ref KAKAROT_ADDRESSES: FieldElement = FieldElement::from_hex_be("0x9001").unwrap(); // Safe unwrap, 0x9001
    pub static ref BLOCKHASH_REGISTRY_ADDRESS: FieldElement = FieldElement::from_hex_be("0x9002").unwrap(); // Safe unwrap, 0x9002
}

/// Convert Hive Genesis Config to Madara Genesis Config
///
/// This function will:
/// 1. Load the Madara genesis file
/// 2. Compute the class hash of Kakarot contracts
/// 3. Add Kakarot contracts to Loader
/// 4. Add Hive accounts to Loader (fund, storage, bytecode, proxy implementation)
/// 5. Serialize Loader to Madara genesis file
pub async fn serialize_hive_to_madara_genesis_config(
    hive_genesis: HiveGenesisConfig,
    mut madara_loader: GenesisLoader,
    combined_genesis: &Path,
    compiled_path: &Path,
) -> Result<(), IoError> {
    // Compute the class hash of Kakarot contracts
    let class_hashes = compute_kakarot_contracts_class_hash();

    // { contract : class_hash }
    let mut kakarot_contracts = HashMap::<String, FieldElement>::new();

    // Add Kakarot contracts Contract Classes to loader
    // Vec so no need to sort
    class_hashes.iter().for_each(|(filename, class_hash)| {
        madara_loader.contract_classes.push((
            HexFelt(*class_hash),
            ContractClass::Path {
                // Add the compiled path to the Kakarot contract filename
                path: compiled_path.join(filename).with_extension("json").into_os_string().into_string().unwrap(), /* safe unwrap,
                                                                                             * valid path */
                version: 0,
            },
        ));

        // Add Kakarot contracts {contract : class_hash} to Kakarot Contracts HashMap
        // Remove .json from filename to get contract name
        kakarot_contracts.insert(filename.to_string(), *class_hash);
    });

    // Set the Kakarot contracts address and proxy class hash
    let account_proxy_class_hash = *kakarot_contracts.get("proxy").expect("Failed to get proxy class hash");
    let contract_account_class_hash =
        *kakarot_contracts.get("contract_account").expect("Failed to get contract_account class hash");
    let eoa_class_hash = *kakarot_contracts.get("externally_owned_account").expect("Failed to get eoa class hash");

    // Add Kakarot contracts to Loader
    madara_loader.contracts.push((
        HexFelt(*KAKAROT_ADDRESSES),
        HexFelt(*kakarot_contracts.get("kakarot").expect("Failed to get kakarot class hash")),
    ));
    madara_loader.contracts.push((
        HexFelt(*BLOCKHASH_REGISTRY_ADDRESS),
        HexFelt(*kakarot_contracts.get("blockhash_registry").expect("Failed to get blockhash_registry class hash")),
    ));

    // Set storage keys of Kakarot contract
    // https://github.com/kkrt-labs/kakarot/blob/main/src/kakarot/constants.cairo
    let storage_keys = [
        ("native_token_address", FieldElement::from_hex_be(STARKNET_NATIVE_TOKEN).unwrap()),
        ("contract_account_class_hash", contract_account_class_hash),
        ("externally_owned_account", eoa_class_hash),
        ("account_proxy_class_hash", account_proxy_class_hash),
        ("blockhash_registry_address", *BLOCKHASH_REGISTRY_ADDRESS),
    ];

    storage_keys.iter().for_each(|(key, value)| {
        let storage_tuple = genesis_set_storage_starknet_contract(*KAKAROT_ADDRESSES, key, &[], *value, 0);
        madara_loader
            .storage
            .push(unsafe { std::mem::transmute::<((Felt, Felt), Felt), ((HexFelt, HexFelt), HexFelt)>(storage_tuple) });
    });

    // Add Hive accounts to loader
    // Convert the EVM accounts to Starknet accounts using compute_starknet_address
    // Sort by key to ensure deterministic order
    let mut hive_accounts: Vec<(reth_primitives::H160, AccountInfo)> = hive_genesis.alloc.into_iter().collect();
    hive_accounts.sort_by_key(|(address, _)| *address);
    hive_accounts.iter().for_each(|(evm_address, account_info)| {
        // Use the given Kakarot contract address and declared proxy class hash for compute_starknet_address
        let starknet_address = compute_starknet_address(
            *KAKAROT_ADDRESSES,
            account_proxy_class_hash,
            FieldElement::from_byte_slice_be(evm_address.as_bytes()).unwrap(), /* safe unwrap since evm_address
                                                                                * is 20 bytes */
        );
        // Push to contracts
        madara_loader.contracts.push((HexFelt(starknet_address), HexFelt(account_proxy_class_hash)));

        // Set the balance of the account
        // Call genesis_fund_starknet_address util to get the storage tuples
        let balance_storage_tuples = genesis_fund_starknet_address(starknet_address, account_info.balance);
        balance_storage_tuples.iter().for_each(|balance_storage_tuple| {
            madara_loader.storage.push(unsafe {
                std::mem::transmute::<((Felt, Felt), Felt), ((HexFelt, HexFelt), HexFelt)>(*balance_storage_tuple)
            });
        });

        // Set the storage of the account, if any
        if let Some(storage) = account_info.storage.as_ref() {
            let mut storage: Vec<(U256, U256)> = storage.iter().map(|(k, v)| (*k, *v)).collect();
            storage.sort_by_key(|(key, _)| *key);
            storage.iter().for_each(|(key, value)| {
                // Call genesis_set_storage_kakarot_contract_account util to get the storage tuples
                let storage_tuples = genesis_set_storage_kakarot_contract_account(starknet_address, *key, *value);
                storage_tuples.iter().for_each(|storage_tuples| {
                    madara_loader.storage.push(unsafe {
                        std::mem::transmute::<((Felt, Felt), Felt), ((HexFelt, HexFelt), HexFelt)>(*storage_tuples)
                    });
                });
            });
        }

        // Determine the proxy implementation class hash based on whether bytecode is present
        // Set the bytecode to the storage of the account, if any
        let proxy_implementation_class_hash = if let Some(bytecode) = account_info.code.as_ref() {
            // Call genesis_set_code_kakarot_contract_account util to get the storage tuples
            let code_storage_tuples = genesis_set_bytecode(bytecode, starknet_address);
            // Set the bytecode of the account
            madara_loader.storage.extend(code_storage_tuples.iter().map(|code_storage_tuple| unsafe {
                std::mem::transmute::<((Felt, Felt), Felt), ((HexFelt, HexFelt), HexFelt)>(*code_storage_tuple)
            }));

            // Since it has bytecode, it's a contract account
            contract_account_class_hash
        } else {
            // Since it has no bytecode, it's an externally owned account
            eoa_class_hash
        };

        // Set the proxy implementation of the account to the determined class hash
        let proxy_implementation_storage_tuples = genesis_set_storage_starknet_contract(
            starknet_address,
            "_implementation",
            &[],
            proxy_implementation_class_hash,
            0, // 0 since it's storage value is felt
        );
        madara_loader.storage.push(unsafe {
            std::mem::transmute::<((Felt, Felt), Felt), ((HexFelt, HexFelt), HexFelt)>(
                proxy_implementation_storage_tuples,
            )
        });
    });

    // Serialize the loader to a string
    let madara_genesis_str = serde_json::to_string_pretty(&madara_loader)?;
    // Write the string to a file
    fs::write(combined_genesis, madara_genesis_str)?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub chain_id: i128,
    pub homestead_block: i128,
    pub eip150_block: i128,
    pub eip150_hash: H256,
    pub eip155_block: i128,
    pub eip158_block: i128,
}

#[derive(Serialize, Deserialize)]
pub struct AccountInfo {
    pub balance: U256,
    pub code: Option<Bytes>,
    pub storage: Option<HashMap<U256, U256>>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pallet_starknet::genesis_loader::GenesisLoader;
    use reth_primitives::U256;

    use super::*;

    #[test]
    fn test_read_hive_genesis() {
        // Read the hive genesis file
        let genesis = HiveGenesisConfig::from_file("./src/test_data/hive_genesis.json").unwrap();

        // Verify the genesis file has the expected number of accounts
        assert_eq!(genesis.alloc.len(), 7);

        // Verify balance of each account is not empty
        assert!(genesis.alloc.values().all(|account_info| account_info.balance >= U256::from(0)));

        // Verify the storage field for each account
        // Since there is only one account with non-empty storage, we can hardcode the expected values
        assert!(genesis.alloc.values().all(|account_info| {
            account_info.storage.as_ref().map_or(true, |storage| {
                storage.len() == 2
                    && *storage
                        .get(
                            &U256::from_str("0x0000000000000000000000000000000000000000000000000000000000000000")
                                .unwrap(),
                        )
                        .unwrap()
                        == U256::from_str("0x1234").unwrap()
                    && *storage
                        .get(
                            &U256::from_str("0x6661e9d6d8b923d5bbaab1b96e1dd51ff6ea2a93520fdc9eb75d059238b8c5e9")
                                .unwrap(),
                        )
                        .unwrap()
                        == U256::from_str("0x01").unwrap()
            })
        }));

        // Verify the code field for each account, if exists, is not empty
        assert!(
            genesis.alloc.values().all(|account_info| account_info.code.as_ref().map_or(true, |code| !code.is_empty()))
        );
    }

    #[tokio::test]
    async fn test_madara_genesis() {
        // Given
        let hive_genesis = HiveGenesisConfig::from_file("./src/test_data/hive_genesis.json").unwrap();
        let madara_loader =
            serde_json::from_str::<GenesisLoader>(std::include_str!("../test_data/madara_genesis.json")).unwrap();
        let combined_genesis = Path::new("./src/test_data/combined_genesis.json");
        let compiled_path = Path::new("./cairo-contracts/build");

        // When
        serialize_hive_to_madara_genesis_config(hive_genesis, madara_loader, combined_genesis, compiled_path)
            .await
            .unwrap();

        // Then
        let combined_genesis = fs::read_to_string("./src/test_data/combined_genesis.json").unwrap();
        let loader: GenesisLoader =
            serde_json::from_str(&combined_genesis).expect("Failed to read combined_genesis.json");
        assert_eq!(9 + 2 + 7, loader.contracts.len()); // 9 original + 2 Kakarot contracts + 7 hive

        // After
        fs::remove_file("./src/test_data/combined_genesis.json").unwrap();
    }
}
