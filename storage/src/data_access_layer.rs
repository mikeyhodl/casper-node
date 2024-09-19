use crate::global_state::{
    error::Error as GlobalStateError,
    state::{CommitProvider, StateProvider},
};
use casper_types::{execution::Effects, Digest, EraId};

use crate::tracking_copy::TrackingCopy;

mod addressable_entity;
pub mod auction;
pub mod balance;
mod balance_hold;
pub mod bids;
mod block_global;
pub mod block_rewards;
mod entry_points;
pub mod era_validators;
mod execution_results_checksum;
mod fee;
mod flush;
pub mod forced_undelegate;
mod genesis;
pub mod handle_fee;
mod handle_refund;
mod key_prefix;
pub mod mint;
pub mod prefixed_values;
mod protocol_upgrade;
pub mod prune;
pub mod query;
mod round_seigniorage;
mod seigniorage_recipients;
pub mod step;
mod system_entity_registry;
pub mod tagged_values;
mod total_supply;
mod trie;

pub use addressable_entity::{AddressableEntityRequest, AddressableEntityResult};
pub use auction::{AuctionMethod, BiddingRequest, BiddingResult};
pub use balance::{
    BalanceHolds, BalanceHoldsWithProof, BalanceIdentifier, BalanceRequest, BalanceResult,
    GasHoldBalanceHandling, ProofHandling, ProofsResult,
};
pub use balance_hold::{
    BalanceHoldError, BalanceHoldKind, BalanceHoldMode, BalanceHoldRequest, BalanceHoldResult,
    InsufficientBalanceHandling,
};
pub use bids::{BidsRequest, BidsResult};
pub use block_global::{BlockGlobalKind, BlockGlobalRequest, BlockGlobalResult};
pub use block_rewards::{BlockRewardsError, BlockRewardsRequest, BlockRewardsResult};
pub use entry_points::{EntryPointsRequest, EntryPointsResult};
pub use era_validators::{EraValidatorsRequest, EraValidatorsResult};
pub use execution_results_checksum::{
    ExecutionResultsChecksumRequest, ExecutionResultsChecksumResult,
    EXECUTION_RESULTS_CHECKSUM_NAME,
};
pub use fee::{FeeError, FeeRequest, FeeResult};
pub use flush::{FlushRequest, FlushResult};
pub use genesis::{GenesisRequest, GenesisResult};
pub use handle_fee::{HandleFeeMode, HandleFeeRequest, HandleFeeResult};
pub use handle_refund::{HandleRefundMode, HandleRefundRequest, HandleRefundResult};
pub use key_prefix::KeyPrefix;
pub use mint::{TransferRequest, TransferResult};
pub use protocol_upgrade::{ProtocolUpgradeRequest, ProtocolUpgradeResult};
pub use prune::{PruneRequest, PruneResult};
pub use query::{QueryRequest, QueryResult};
pub use round_seigniorage::{RoundSeigniorageRateRequest, RoundSeigniorageRateResult};
pub use seigniorage_recipients::{SeigniorageRecipientsRequest, SeigniorageRecipientsResult};
pub use step::{EvictItem, RewardItem, SlashItem, StepError, StepRequest, StepResult};
pub use system_entity_registry::{
    SystemEntityRegistryPayload, SystemEntityRegistryRequest, SystemEntityRegistryResult,
    SystemEntityRegistrySelector,
};
pub use total_supply::{TotalSupplyRequest, TotalSupplyResult};
pub use trie::{PutTrieRequest, PutTrieResult, TrieElement, TrieRequest, TrieResult};

pub struct Block {
    _era_id: EraId,
}

pub trait BlockProvider {
    type Error;

    fn read_block_by_height(&self, _height: usize) -> Result<Option<Block>, Self::Error> {
        // TODO: We need to implement this
        todo!()
    }
}

#[derive(Default, Copy, Clone)]
pub struct BlockStore(());

impl BlockStore {
    pub fn new() -> Self {
        BlockStore(())
    }
}

// We're currently putting it here, but in future it needs to move to its own crate.
#[derive(Copy, Clone)]
pub struct DataAccessLayer<S> {
    pub block_store: BlockStore,
    pub state: S,
    pub max_query_depth: u64,
    pub enable_entity: bool,
}

impl<S> DataAccessLayer<S> {
    pub fn state(&self) -> &S {
        &self.state
    }
}

impl<S> CommitProvider for DataAccessLayer<S>
where
    S: CommitProvider,
{
    fn commit_effects(
        &self,
        state_hash: Digest,
        effects: Effects,
    ) -> Result<Digest, GlobalStateError> {
        self.state.commit_effects(state_hash, effects)
    }

    fn commit_values(
        &self,
        state_hash: Digest,
        values_to_write: Vec<(casper_types::Key, casper_types::StoredValue)>,
        keys_to_prune: std::collections::BTreeSet<casper_types::Key>,
    ) -> Result<Digest, GlobalStateError> {
        self.state
            .commit_values(state_hash, values_to_write, keys_to_prune)
    }
}

impl<S> StateProvider for DataAccessLayer<S>
where
    S: StateProvider,
{
    type Reader = S::Reader;

    fn flush(&self, request: FlushRequest) -> FlushResult {
        self.state.flush(request)
    }

    fn empty_root(&self) -> Digest {
        self.state.empty_root()
    }

    fn tracking_copy(
        &self,
        hash: Digest,
    ) -> Result<Option<TrackingCopy<S::Reader>>, GlobalStateError> {
        match self.state.checkout(hash)? {
            Some(reader) => Ok(Some(TrackingCopy::new(
                reader,
                self.max_query_depth,
                self.enable_entity,
            ))),
            None => Ok(None),
        }
    }

    fn checkout(&self, state_hash: Digest) -> Result<Option<Self::Reader>, GlobalStateError> {
        self.state.checkout(state_hash)
    }

    fn trie(&self, request: TrieRequest) -> TrieResult {
        self.state.trie(request)
    }

    fn put_trie(&self, request: PutTrieRequest) -> PutTrieResult {
        self.state.put_trie(request)
    }

    fn missing_children(&self, trie_raw: &[u8]) -> Result<Vec<Digest>, GlobalStateError> {
        self.state.missing_children(trie_raw)
    }
}
