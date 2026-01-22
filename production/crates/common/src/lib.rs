//! Common types shared between MPC wallet components.

pub mod bitcoin_tx;
pub mod bitcoin_utils;
pub mod bitcoin_address;
pub mod crypto;
pub mod discovery;
pub mod grant;
pub mod observability;
pub mod protocol;
pub mod selection;
pub mod storage;
pub mod types;

// Re-export specific items to avoid ambiguity
pub use bitcoin_tx::{
    build_unsigned_taproot_transaction, build_unsigned_transaction, finalize_taproot_transaction,
    AddressInfo, BalanceResponse, BitcoinNetwork, BlockchainClient, FeeEstimates,
    SendBitcoinRequest, SendBitcoinResponse, UnsignedTransaction, Utxo,
};
pub use bitcoin_utils::{
    derive_bitcoin_address, derive_bitcoin_address_legacy, derive_bitcoin_address_taproot,
    derive_bitcoin_address_taproot_from_xonly, derive_ethereum_address, DerivedAddress,
    ExtendedPubKey, MpcHdWallet,
};
pub use grant::{GrantError, SigningGrant, DEFAULT_GRANT_VALIDITY_SECS};
pub use observability::{EventType, LogEvent, MetricsSnapshot, ProtocolMetrics, SessionSpan};
pub use protocol::{
    ProcessRoundRequest, ProcessRoundResponse, ProcessSigningRoundRequest,
    ProcessSigningRoundResponse, ProtocolMessage, SignRequest, SignResponse, SignatureData,
    StartDkgRequest, StartDkgResponse, StartSigningRequest, StartSigningResponse,
};
pub use selection::{
    derive_selection_seed, ParticipantSelector, SelectionError, SelectionInput, SelectionPolicy,
    SelectionResult,
};
pub use storage::{
    KeyShareStore, RelaySessionStore, StoredKeyShare, StoredRelaySession, StoredWallet, WalletStore,
};
pub use types::{
    CreateWalletRequest, CreateWalletResponse, MpcWalletError, NodeStatus, WalletType,
};

// Re-export discovery types
pub use discovery::{
    HeartbeatRequest, HeartbeatResponse, ListNodesResponse, NodeCapabilities, NodeHealthMetrics,
    NodeInfo, ProtocolCapability, RegisterNodeRequest, RegisterNodeResponse,
};

// Re-export bitcoin address and crypto utilities
pub use bitcoin_address::{
    derive_p2tr_address, derive_p2wpkh_address, validate_address, AddressType as BtcAddressType,
    BitcoinAddressInfo, BitcoinNetwork as BtcNetwork,
};
pub use crypto::{
    compute_sighash_ecdsa, compute_sighash_taproot, der_to_bitcoin_signature, double_sha256,
    parse_compressed_pubkey, serialize_compressed_pubkey, tagged_hash, verify_ecdsa_signature,
    verify_schnorr_signature, TaprootPrevout,
};

/// Fixed number of MPC nodes.
pub const NUM_PARTIES: u16 = 4;

/// Fixed threshold for signing (t-of-n).
pub const THRESHOLD: u16 = 3;
