//! Transaction Orchestration Service
//!
//! This module provides the main orchestration service that coordinates
//! the complete transaction lifecycle from creation to confirmation.
//!
//! # Security Principles
//!
//! 1. **Defense in Depth**: Multiple validation layers
//! 2. **Least Privilege**: Minimal permissions per component
//! 3. **Fail-Safe Defaults**: System defaults to safe state on errors
//! 4. **Complete Mediation**: Every access checked
//! 5. **Open Design**: Security through cryptography
//! 6. **Separation of Duties**: No single control point
//! 7. **Psychological Acceptability**: Type-safe APIs

pub mod config;
pub mod service;
pub mod timeout_monitor;
pub mod health_checker;
pub mod heartbeat_service;
pub mod error;
pub mod dkg_service;
pub mod aux_info_service;
pub mod presig_service;
pub mod signing_coordinator;
pub mod protocol_router;
pub mod message_router;

pub use config::{OrchestrationConfig, OrchestrationConfigBuilder};
pub use service::{OrchestrationService, OrchestrationServiceBuilder};
pub use timeout_monitor::{TimeoutMonitor, TimeoutMonitorBuilder};
pub use health_checker::{HealthChecker, HealthCheckerBuilder};
pub use heartbeat_service::HeartbeatService;
pub use error::{OrchestrationError, Result};
pub use dkg_service::{DkgService, DkgResult, DkgStatus, DkgCeremony, ProtocolType};
pub use aux_info_service::{AuxInfoService, AuxInfoResult, AuxInfoStatus, AuxInfoCeremony};
pub use presig_service::{PresignatureService, PresignatureStats};
pub use signing_coordinator::{SigningCoordinator, SignatureProtocol, SigningRequest, SignatureShare, CombinedSignature};
pub use protocol_router::{ProtocolRouter, ProtocolSelection, BitcoinAddressType};
pub use message_router::{MessageRouter, ProtocolMessage, ProtocolType as MessageProtocolType};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::config::OrchestrationConfig;
    pub use crate::service::OrchestrationService;
    pub use crate::timeout_monitor::TimeoutMonitor;
    pub use crate::health_checker::HealthChecker;
    pub use crate::error::{OrchestrationError, Result};
}
