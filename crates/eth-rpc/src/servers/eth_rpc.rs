use std::sync::Arc;

use jsonrpsee::core::{async_trait, RpcResult as Result};
use jsonrpsee::types::error::{INTERNAL_ERROR_CODE, METHOD_NOT_FOUND_CODE};
use kakarot_rpc_core::client::api::KakarotEthApi;
use kakarot_rpc_core::client::constants::CHAIN_ID;
use kakarot_rpc_core::client::errors::{rpc_err, EthApiError};
use kakarot_rpc_core::models::block::EthBlockId;
use reth_primitives::rpc::transaction::eip2930::AccessListWithGasUsed;
use reth_primitives::{Address, BlockId, BlockNumberOrTag, Bytes, H256, H64, U128, U256, U64};
use reth_rpc_types::{
    CallRequest, EIP1186AccountProofResponse, FeeHistory, Filter, FilterChanges, Index, Log, RichBlock, SyncStatus,
    Transaction as EtherTransaction, TransactionReceipt, TransactionRequest, Work,
};
use serde_json::Value;
use starknet::core::types::BlockId as StarknetBlockId;
use starknet::providers::Provider;

use crate::api::eth_api::EthApiServer;

/// The RPC module for the Ethereum protocol required by Kakarot.
pub struct KakarotEthRpc<P: Provider + Send + Sync> {
    pub kakarot_client: Arc<dyn KakarotEthApi<P>>,
}

impl<P: Provider + Send + Sync> KakarotEthRpc<P> {
    pub fn new(kakarot_client: Arc<dyn KakarotEthApi<P>>) -> Self {
        Self { kakarot_client }
    }
}

#[async_trait]
impl<P: Provider + Send + Sync + 'static> EthApiServer for KakarotEthRpc<P> {
    async fn block_number(&self) -> Result<U64> {
        let block_number = self.kakarot_client.block_number().await?;
        Ok(block_number)
    }

    async fn syncing(&self) -> Result<SyncStatus> {
        let status = self.kakarot_client.syncing().await?;
        Ok(status)
    }

    async fn author(&self) -> Result<Address> {
        todo!()
    }

    async fn accounts(&self) -> Result<Vec<Address>> {
        Ok(Vec::new())
    }

    async fn chain_id(&self) -> Result<Option<U64>> {
        Ok(Some(CHAIN_ID.into()))
    }

    async fn block_by_hash(&self, hash: H256, full: bool) -> Result<Option<RichBlock>> {
        let block_id = EthBlockId::new(BlockId::Hash(hash.into()));
        let starknet_block_id: StarknetBlockId = block_id.try_into().map_err(EthApiError::<P::Error>::from)?;
        let block = self.kakarot_client.get_eth_block_from_starknet_block(starknet_block_id, full).await?;
        Ok(Some(block))
    }

    async fn block_by_number(&self, number: BlockNumberOrTag, full: bool) -> Result<Option<RichBlock>> {
        let block_id = EthBlockId::new(BlockId::Number(number));
        let starknet_block_id: StarknetBlockId = block_id.try_into().map_err(EthApiError::<P::Error>::from)?;
        let block = self.kakarot_client.get_eth_block_from_starknet_block(starknet_block_id, full).await?;
        Ok(Some(block))
    }

    async fn block_transaction_count_by_hash(&self, hash: H256) -> Result<U64> {
        let transaction_count = self.kakarot_client.block_transaction_count_by_hash(hash).await?;
        Ok(transaction_count)
    }

    async fn block_transaction_count_by_number(&self, number: BlockNumberOrTag) -> Result<U64> {
        let transaction_count = self.kakarot_client.block_transaction_count_by_number(number).await?;
        Ok(transaction_count)
    }

    async fn block_uncles_count_by_hash(&self, _hash: H256) -> Result<U256> {
        todo!()
    }

    async fn block_uncles_count_by_number(&self, _number: BlockNumberOrTag) -> Result<U256> {
        todo!()
    }

    async fn uncle_by_block_hash_and_index(&self, _hash: H256, _index: Index) -> Result<Option<RichBlock>> {
        todo!()
    }

    async fn uncle_by_block_number_and_index(
        &self,
        _number: BlockNumberOrTag,
        _index: Index,
    ) -> Result<Option<RichBlock>> {
        todo!()
    }

    async fn transaction_by_hash(&self, _hash: H256) -> Result<Option<EtherTransaction>> {
        let ether_tx = self.kakarot_client.transaction_by_hash(_hash).await?;
        Ok(ether_tx)
    }

    async fn transaction_by_block_hash_and_index(&self, hash: H256, index: Index) -> Result<Option<EtherTransaction>> {
        let block_id = BlockId::Hash(hash.into());
        let tx = self.kakarot_client.transaction_by_block_id_and_index(block_id, index).await?;
        Ok(Some(tx))
    }

    async fn transaction_by_block_number_and_index(
        &self,
        number: BlockNumberOrTag,
        index: Index,
    ) -> Result<Option<EtherTransaction>> {
        let block_id = BlockId::Number(number);
        let tx = self.kakarot_client.transaction_by_block_id_and_index(block_id, index).await?;
        Ok(Some(tx))
    }

    async fn transaction_receipt(&self, hash: H256) -> Result<Option<TransactionReceipt>> {
        let receipt = self.kakarot_client.transaction_receipt(hash).await?;
        Ok(receipt)
    }

    async fn balance(&self, address: Address, block_id: Option<BlockId>) -> Result<U256> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let balance = self.kakarot_client.balance(address, block_id).await?;
        Ok(balance)
    }

    async fn storage_at(&self, address: Address, index: U256, block_id: Option<BlockId>) -> Result<U256> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let value = self.kakarot_client.storage_at(address, index, block_id).await?;
        Ok(value)
    }

    async fn transaction_count(&self, address: Address, block_id: Option<BlockId>) -> Result<U256> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));

        let transaction_count = self.kakarot_client.nonce(address, block_id).await?;

        Ok(transaction_count)
    }

    async fn get_code(&self, address: Address, block_id: Option<BlockId>) -> Result<Bytes> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let code = self.kakarot_client.get_code(address, block_id).await?;
        Ok(code)
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        let logs = self.kakarot_client.get_logs(filter).await?;
        Ok(logs)
    }

    async fn call(&self, request: CallRequest, block_id: Option<BlockId>) -> Result<Bytes> {
        // unwrap option or return jsonrpc error
        let to = request.to.ok_or_else(|| {
            rpc_err(INTERNAL_ERROR_CODE, "CallRequest `to` field is None. Cannot process a Kakarot call")
        })?;

        let calldata = request.data.ok_or_else(|| {
            rpc_err(INTERNAL_ERROR_CODE, "CallRequest `data` field is None. Cannot process a Kakarot call")
        })?;

        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let result = self.kakarot_client.call(to, Bytes::from(calldata.0), block_id).await?;

        Ok(result)
    }

    async fn create_access_list(
        &self,
        _request: CallRequest,
        _block_id: Option<BlockId>,
    ) -> Result<AccessListWithGasUsed> {
        todo!()
    }

    async fn estimate_gas(&self, request: CallRequest, block_id: Option<BlockId>) -> Result<U256> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));

        Ok(self.kakarot_client.estimate_gas(request, block_id).await?)
    }

    async fn gas_price(&self) -> Result<U256> {
        let gas_price = self.kakarot_client.base_fee_per_gas();
        Ok(gas_price)
    }

    async fn fee_history(
        &self,
        block_count: U256,
        newest_block: BlockNumberOrTag,
        reward_percentiles: Option<Vec<f64>>,
    ) -> Result<FeeHistory> {
        let fee_history = self.kakarot_client.fee_history(block_count, newest_block, reward_percentiles).await?;

        Ok(fee_history)
    }

    async fn max_priority_fee_per_gas(&self) -> Result<U128> {
        let max_priority_fee = self.kakarot_client.max_priority_fee_per_gas();
        Ok(max_priority_fee)
    }

    async fn is_mining(&self) -> Result<bool> {
        Err(rpc_err(METHOD_NOT_FOUND_CODE, "Unsupported method: eth_mining. See available methods at https://github.com/sayajin-labs/kakarot-rpc/blob/main/docs/rpc_api_status.md".to_string()))
    }

    async fn hashrate(&self) -> Result<U256> {
        Err(rpc_err(METHOD_NOT_FOUND_CODE, "Unsupported method: eth_hashrate. See available methods at https://github.com/sayajin-labs/kakarot-rpc/blob/main/docs/rpc_api_status.md".to_string()))
    }

    async fn get_work(&self) -> Result<Work> {
        Err(rpc_err(METHOD_NOT_FOUND_CODE, "Unsupported method: eth_getWork. See available methods at https://github.com/sayajin-labs/kakarot-rpc/blob/main/docs/rpc_api_status.md".to_string()))
    }

    async fn submit_hashrate(&self, _hashrate: U256, _id: H256) -> Result<bool> {
        todo!()
    }

    async fn submit_work(&self, _nonce: H64, _pow_hash: H256, _mix_digest: H256) -> Result<bool> {
        todo!()
    }

    async fn send_transaction(&self, _request: TransactionRequest) -> Result<H256> {
        todo!()
    }

    async fn send_raw_transaction(&self, bytes: Bytes) -> Result<H256> {
        let transaction_hash = self.kakarot_client.send_transaction(bytes).await?;
        Ok(transaction_hash)
    }

    async fn sign(&self, _address: Address, _message: Bytes) -> Result<Bytes> {
        todo!()
    }

    async fn sign_transaction(&self, _transaction: CallRequest) -> Result<Bytes> {
        todo!()
    }

    async fn sign_typed_data(&self, _address: Address, _data: Value) -> Result<Bytes> {
        todo!()
    }

    async fn get_proof(
        &self,
        _address: Address,
        _keys: Vec<H256>,
        _block_id: Option<BlockId>,
    ) -> Result<EIP1186AccountProofResponse> {
        todo!()
    }

    async fn new_filter(&self, _filter: Filter) -> Result<U64> {
        todo!()
    }

    async fn new_block_filter(&self) -> Result<U64> {
        todo!()
    }

    async fn new_pending_transaction_filter(&self) -> Result<U64> {
        todo!()
    }

    async fn uninstall_filter(&self, _id: U64) -> Result<bool> {
        todo!()
    }

    async fn get_filter_changes(&self, _id: U64) -> Result<FilterChanges> {
        todo!()
    }

    async fn get_filter_logs(&self, _id: U64) -> Result<FilterChanges> {
        todo!()
    }
}
