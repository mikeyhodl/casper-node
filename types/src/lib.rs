//! Types used to allow creation of Wasm contracts and tests for use on the Casper Platform.

#![cfg_attr(
    not(any(
        feature = "json-schema",
        feature = "datasize",
        feature = "std",
        feature = "testing",
        test,
    )),
    no_std
)]
#![doc(html_root_url = "https://docs.rs/casper-types/3.0.0")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/CasperLabs/casper-node/master/images/CasperLabs_Logo_Favicon_RGB_50px.png",
    html_logo_url = "https://raw.githubusercontent.com/CasperLabs/casper-node/master/images/CasperLabs_Logo_Symbol_RGB.png",
    test(attr(deny(warnings)))
)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg_attr(not(test), macro_use)]
extern crate alloc;
extern crate core;

mod access_rights;
pub mod account;
pub mod addressable_entity;
pub mod api_error;
mod block;
mod block_time;
pub mod bytesrepr;
#[cfg(any(feature = "std", test))]
mod chainspec;
pub mod checksummed_hex;
mod cl_type;
#[cfg(not(any(feature = "sdk")))]
mod cl_value;
#[cfg(feature = "sdk")]
pub mod cl_value;
mod contract_wasm;
pub mod contracts;
pub mod crypto;
mod deploy;
mod deploy_info;
mod digest;
mod display_iter;
mod era_id;
pub mod execution;
#[cfg(any(feature = "std", test))]
#[cfg(feature = "std")]
pub mod file_utils;
mod gas;
#[cfg(any(feature = "testing", feature = "gens", test))]
pub mod gens;
mod json_pretty_printer;
mod key;
mod motes;
pub mod package;
mod phase;
mod protocol_version;
mod semver;
pub(crate) mod serde_helpers;
mod stored_value;
pub mod system;
mod tagged;
#[cfg(any(feature = "testing", test))]
pub mod testing;
mod timestamp;
mod transfer;
mod transfer_result;
mod uint;
mod uref;

#[cfg(feature = "std")]
#[cfg(not(any(feature = "sdk")))]
use libc::{c_long, sysconf, _SC_PAGESIZE};
#[cfg(feature = "std")]
use once_cell::sync::Lazy;

pub use crate::uint::{UIntParseError, U128, U256, U512};
pub use access_rights::{
    AccessRights, ContextAccessRights, GrantedAccess, ACCESS_RIGHTS_SERIALIZED_LENGTH,
};
#[doc(inline)]
pub use addressable_entity::{
    AddressableEntity, ContractHash, EntryPoint, EntryPointAccess, EntryPointType, EntryPoints,
    Parameter,
};
#[doc(inline)]
pub use api_error::ApiError;
pub use block::{
    Block, BlockBody, BlockHash, BlockHashAndHeight, BlockHeader, BlockSignatures,
    BlockSignaturesMergeError, BlockValidationError, EraEnd, EraReport, FinalitySignature,
    FinalitySignatureId, SignedBlockHeader, SignedBlockHeaderValidationError,
};
#[cfg(all(feature = "std", feature = "json-schema"))]
pub use block::{
    JsonBlock, JsonBlockBody, JsonBlockHeader, JsonEraEnd, JsonEraReport, JsonProof, JsonReward,
    JsonValidatorWeight,
};
pub use block_time::{BlockTime, BLOCKTIME_SERIALIZED_LENGTH};
#[cfg(any(feature = "std", test))]
pub use chainspec::{
    AccountConfig, AccountsConfig, ActivationPoint, AuctionCosts, BrTableCost, Chainspec,
    ChainspecRawBytes, ChainspecRegistry, ConsensusProtocolName, ControlFlowCosts, CoreConfig,
    DelegatorConfig, DeployConfig, GenesisAccount, GenesisValidator, GlobalStateUpdate,
    GlobalStateUpdateConfig, GlobalStateUpdateError, HandlePaymentCosts, HighwayConfig,
    HostFunction, HostFunctionCost, HostFunctionCosts, LegacyRequiredFinality, MintCosts,
    NetworkConfig, OpcodeCosts, ProtocolConfig, StandardPaymentCosts, StorageCosts, SystemConfig,
    UpgradeConfig, ValidatorConfig, WasmConfig,
};
#[cfg(any(all(feature = "std", feature = "testing"), test))]
pub use chainspec::{
    DEFAULT_ADD_BID_COST, DEFAULT_ADD_COST, DEFAULT_BIT_COST, DEFAULT_CONST_COST,
    DEFAULT_CONTROL_FLOW_BLOCK_OPCODE, DEFAULT_CONTROL_FLOW_BR_IF_OPCODE,
    DEFAULT_CONTROL_FLOW_BR_OPCODE, DEFAULT_CONTROL_FLOW_BR_TABLE_MULTIPLIER,
    DEFAULT_CONTROL_FLOW_BR_TABLE_OPCODE, DEFAULT_CONTROL_FLOW_CALL_INDIRECT_OPCODE,
    DEFAULT_CONTROL_FLOW_CALL_OPCODE, DEFAULT_CONTROL_FLOW_DROP_OPCODE,
    DEFAULT_CONTROL_FLOW_ELSE_OPCODE, DEFAULT_CONTROL_FLOW_END_OPCODE,
    DEFAULT_CONTROL_FLOW_IF_OPCODE, DEFAULT_CONTROL_FLOW_LOOP_OPCODE,
    DEFAULT_CONTROL_FLOW_RETURN_OPCODE, DEFAULT_CONTROL_FLOW_SELECT_OPCODE,
    DEFAULT_CONVERSION_COST, DEFAULT_CURRENT_MEMORY_COST, DEFAULT_DELEGATE_COST, DEFAULT_DIV_COST,
    DEFAULT_GLOBAL_COST, DEFAULT_GROW_MEMORY_COST, DEFAULT_HOST_FUNCTION_NEW_DICTIONARY,
    DEFAULT_INTEGER_COMPARISON_COST, DEFAULT_LOAD_COST, DEFAULT_LOCAL_COST,
    DEFAULT_MAX_STACK_HEIGHT, DEFAULT_MUL_COST, DEFAULT_NEW_DICTIONARY_COST, DEFAULT_NOP_COST,
    DEFAULT_STORE_COST, DEFAULT_TRANSFER_COST, DEFAULT_UNREACHABLE_COST,
    DEFAULT_WASMLESS_TRANSFER_COST, DEFAULT_WASM_MAX_MEMORY, MAX_PAYMENT_AMOUNT,
};
pub use cl_type::{named_key_type, CLType, CLTyped};
#[cfg(feature = "sdk")]
pub use cl_value::cl_value_to_json;
pub use cl_value::{CLTypeMismatch, CLValue, CLValueError};
pub use contract_wasm::{ContractWasm, ContractWasmHash};
#[doc(inline)]
pub use contracts::Contract;
pub use crypto::*;
pub use deploy::{
    runtime_args, Approval, ApprovalsHash, ContractIdentifier, ContractPackageIdentifier, Deploy,
    DeployConfigurationFailure, DeployDecodeFromJsonError, DeployError, DeployExcessiveSizeError,
    DeployFootprint, DeployHash, DeployHeader, DeployId, ExecutableDeployItem,
    ExecutableDeployItemIdentifier, TransferTarget,
};
#[cfg(any(feature = "std", test))]
pub use deploy::{DeployBuilder, DeployBuilderError};
pub use deploy_info::DeployInfo;
pub use digest::{
    ChunkWithProof, ChunkWithProofVerificationError, Digest, DigestError, IndexedMerkleProof,
    MerkleConstructionError, MerkleVerificationError,
};
pub use display_iter::DisplayIter;
pub use era_id::EraId;
pub use gas::Gas;
pub use json_pretty_printer::json_pretty_print;
#[doc(inline)]
pub use key::{
    DictionaryAddr, FromStrError as KeyFromStrError, HashAddr, Key, KeyTag, BLAKE2B_DIGEST_LENGTH,
    DICTIONARY_ITEM_KEY_MAX_LENGTH, KEY_DICTIONARY_LENGTH, KEY_HASH_LENGTH,
};
pub use motes::Motes;
pub use package::{
    ContractPackageHash, ContractVersion, ContractVersionKey, ContractVersions, Group, Groups,
    Package,
};
pub use phase::{Phase, PHASE_SERIALIZED_LENGTH};
pub use protocol_version::{ProtocolVersion, VersionCheckResult};
#[doc(inline)]
pub use runtime_args::{NamedArg, RuntimeArgs};
pub use semver::{ParseSemVerError, SemVer, SEM_VER_SERIALIZED_LENGTH};
pub use stored_value::{StoredValue, TypeMismatch as StoredValueTypeMismatch};
pub use tagged::Tagged;
#[cfg(any(feature = "std", test))]
pub use timestamp::serde_option_time_diff;
pub use timestamp::{TimeDiff, Timestamp};
pub use transfer::{
    FromStrError as TransferFromStrError, Transfer, TransferAddr, TRANSFER_ADDR_LENGTH,
};
pub use transfer_result::{TransferResult, TransferredTo};
pub use uref::{
    FromStrError as URefFromStrError, URef, URefAddr, UREF_ADDR_LENGTH, UREF_SERIALIZED_LENGTH,
};

/// OS page size.
#[cfg(feature = "std")]
#[cfg(not(any(feature = "sdk")))]
pub static OS_PAGE_SIZE: Lazy<usize> = Lazy::new(|| {
    /// Sensible default for many if not all systems.
    const DEFAULT_PAGE_SIZE: usize = 4096;

    // https://www.gnu.org/software/libc/manual/html_node/Sysconf.html
    let value: c_long = unsafe { sysconf(_SC_PAGESIZE) };
    if value <= 0 {
        DEFAULT_PAGE_SIZE
    } else {
        value as usize
    }
});
