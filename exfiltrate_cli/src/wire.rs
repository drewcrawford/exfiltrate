// SPDX-License-Identifier: MIT OR Apache-2.0
//! Networking layer for the exfiltrate CLI.
//!
//! This module handles the TCP connection to the debugged application,
//! including RPC serialization/deserialization and progress reporting
//! for large transfers.

pub mod client;
