//! Message types for inter-component communication within exfiltrate.
//!
//! This module defines the core message types used for communication between
//! different parts of the exfiltrate system, particularly for JSON-RPC message
//! routing through proxies and between client/server components.
//!
//! # Overview
//!
//! The module provides a unified message type that can represent different kinds
//! of JSON-RPC messages, allowing components to handle both request/response
//! patterns and one-way notifications in a type-safe manner.
//!
//! # Usage Patterns
//!
//! `SendMessage` is typically used in message queues and channels where different
//! types of JSON-RPC messages need to be sent through the same communication path.
//! This is particularly useful in proxy implementations that need to forward
//! messages without necessarily understanding their content.


/// Represents a message that can be sent through the exfiltrate system.
///
/// This enum provides a unified type for different kinds of JSON-RPC messages,
/// allowing them to be handled uniformly in message queues, channels, and
/// proxy implementations.
///
/// # Variants
///
/// * `Request` - A JSON-RPC request that expects a response. Contains an ID
///   for correlating responses with requests.
/// * `Notification` - A JSON-RPC notification that does not expect a response.
///   Used for one-way communication like logging or status updates.
///
/// # Design Rationale
///
/// This enum enables type-safe message passing while maintaining the flexibility
/// to handle different message patterns. Components can pattern match on the
/// message type to provide appropriate handling for each case.
///

pub enum SendMessage {
    /// A JSON-RPC request message that expects a response.
    /// 
    /// Requests include an ID field for response correlation and are used
    /// for bidirectional communication patterns where the sender needs to
    /// receive a result or error response.
    Request(crate::jrpc::Request),
    
    /// A JSON-RPC notification message that does not expect a response.
    /// 
    /// Notifications are used for one-way communication such as logging,
    /// status updates, or events that don't require acknowledgment.
    Notification(crate::jrpc::Notification),
}

