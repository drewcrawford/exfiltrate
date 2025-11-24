//! Internal implementation details for the exfiltrate debugging framework.
//!
//! This crate provides the core types and wire protocol used by the exfiltrate system.
//! It is not intended for direct use; users should depend on the `exfiltrate` crate instead.

/// Command trait and response types for the exfiltrate system.
pub mod command;
/// Built-in command implementations.
pub mod commands;
/// Remote procedure call protocol types.
pub mod rpc;
/// Wire protocol for TCP/WebSocket communication.
pub mod wire;
