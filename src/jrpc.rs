// SPDX-License-Identifier: MIT OR Apache-2.0
//! JSON-RPC 2.0 protocol implementation.
//!
//! This module provides a complete implementation of the JSON-RPC 2.0 protocol,
//! supporting requests, responses, notifications, and error handling. It is designed
//! to be used for inter-process communication, particularly in the context of the
//! Model Context Protocol (MCP) and similar RPC-based systems.
//!
//! # Overview
//!
//! JSON-RPC is a remote procedure call protocol encoded in JSON. This implementation
//! follows the [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification)
//! and provides a lightweight, thread-safe solution without requiring async runtimes
//! like tokio.
//!
//! The module provides four main types:
//! - [`Request`]: Represents a method call that expects a response
//! - [`Response`]: Represents the result of a method call
//! - [`Notification`]: Represents a method call that doesn't expect a response
//! - [`Error`]: Represents an error that occurred during method execution
//!
//! # Protocol Details
//!
//! JSON-RPC 2.0 messages are JSON objects that contain:
//! - A `jsonrpc` field with the value `"2.0"`
//! - Method information (`method` field)
//! - Optional parameters (`params` field)
//! - An identifier (`id` field) for requests/responses (absent for notifications)
//!

use serde::Serialize;
use std::fmt::{Display, Formatter};

/// A JSON-RPC 2.0 request.
///
/// Represents a method call that expects a response. Each request must have
/// a unique `id` that will be included in the corresponding response.
///
/// # Fields
///
/// * `jsonrpc` - Protocol version, must be "2.0"
/// * `method` - The name of the method to be invoked
/// * `params` - Optional parameters for the method (can be an array or object)
/// * `id` - Unique identifier for this request (string, number, or null)
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Request {
    /// The JSON-RPC protocol version (must be "2.0")
    pub jsonrpc: String,
    /// The name of the method to invoke
    pub method: String,
    /// Optional parameters for the method call
    pub params: Option<serde_json::Value>,
    /// Unique identifier for this request
    pub id: serde_json::Value,
}

impl Request {
    /// Creates a new JSON-RPC 2.0 request.
    ///
    /// # Arguments
    ///
    /// * `method` - The name of the method to invoke
    /// * `params` - Optional parameters for the method call
    /// * `id` - Unique identifier for this request
    ///
    #[cfg(feature = "transit")]
    pub fn new(method: String, params: Option<serde_json::Value>, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params,
            id,
        }
    }
}

/// A JSON-RPC 2.0 notification.
///
/// Represents a method call that does not expect a response. Notifications
/// are fire-and-forget messages used for one-way communication such as
/// logging, status updates, or event notifications.
///
/// # Fields
///
/// * `jsonrpc` - Protocol version, must be "2.0"
/// * `method` - The name of the method to be invoked
/// * `params` - Optional parameters for the method (can be an array or object)
///
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Notification {
    /// The JSON-RPC protocol version (must be "2.0")
    pub jsonrpc: String,
    /// The name of the method to invoke
    pub method: String,
    /// Optional parameters for the method call
    pub params: Option<serde_json::Value>,
}

impl Notification {
    /// Creates a new JSON-RPC 2.0 notification.
    ///
    /// # Arguments
    ///
    /// * `method` - The name of the method to invoke
    /// * `params` - Optional parameters for the method call
    ///
    pub fn new(method: String, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        }
    }
}

/// A JSON-RPC 2.0 response.
///
/// Represents the result of a method call. A response contains either
/// a `result` field with the return value or an `error` field with error
/// information, but never both.
///
/// The generic type parameter `R` represents the type of the successful result.
/// This allows for type-safe responses when the result type is known at compile time.
///
/// # Fields
///
/// * `jsonrpc` - Protocol version, must be "2.0"
/// * `result` - The result of the method call (present on success)
/// * `error` - Error information (present on failure)
/// * `id` - The same id that was in the corresponding request
///
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Response<R> {
    /// The JSON-RPC protocol version (must be "2.0")
    pub jsonrpc: String,
    /// The result of the method call (mutually exclusive with error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
    /// Error information if the method call failed (mutually exclusive with result)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    /// The same identifier that was in the request
    pub id: serde_json::Value,
}

impl<R> Response<R> {
    /// Creates a successful response with the given result.
    ///
    /// # Arguments
    ///
    /// * `result` - The successful result of the method call
    /// * `id` - The request identifier to include in the response
    ///
    pub fn new(result: R, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    /// Creates an error response with the given error.
    ///
    /// # Arguments
    ///
    /// * `e` - The error that occurred
    /// * `id` - The request identifier to include in the response
    ///
    pub fn err(e: Error, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(e),
            id,
        }
    }

    /// Converts a typed response into a response with `serde_json::Value` result.
    ///
    /// This is useful when you need to work with responses of different types
    /// in a uniform way, such as when dispatching multiple method handlers that
    /// return different result types. The method preserves error responses unchanged.
    ///
    /// # Panics
    ///
    /// Panics if the result cannot be serialized to JSON. This should only happen
    /// if the type contains non-serializable fields.
    ///
    pub fn erase(self) -> Response<serde_json::Value>
    where
        R: Serialize,
    {
        Response {
            jsonrpc: self.jsonrpc,
            result: self.result.map(|r| serde_json::to_value(r).unwrap()),
            error: self.error,
            id: self.id,
        }
    }
}

/// A JSON-RPC 2.0 error object.
///
/// Represents an error that occurred during the processing of a request.
/// Error codes follow the JSON-RPC 2.0 specification for standard errors.
///
/// # Standard Error Codes
///
/// * `-32700` - Parse error (Invalid JSON)
/// * `-32600` - Invalid Request
/// * `-32601` - Method not found
/// * `-32602` - Invalid params
/// * `-32603` - Internal error
/// * `-32000` to `-32099` - Server error (reserved for implementation-defined errors)
///
/// # Fields
///
/// * `code` - A number indicating the error type
/// * `message` - A string providing a short description of the error
/// * `data` - Optional additional information about the error
///
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Error {
    /// Error code as defined in JSON-RPC 2.0 specification
    pub code: i32,
    /// Human-readable error message
    pub message: String,
    /// Optional additional information about the error
    pub data: Option<serde_json::Value>,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Creates a new error with a custom code and message.
    ///
    /// Use this for application-specific errors. Error codes from -32000 to -32099
    /// are reserved for implementation-defined server errors.
    ///
    /// # Arguments
    ///
    /// * `code` - The error code (should be in range -32000 to -32099 for server errors)
    /// * `message` - Human-readable error message
    /// * `data` - Optional additional error information
    ///
    #[cfg(feature = "transit")]
    pub fn new(code: i32, message: String, data: Option<serde_json::Value>) -> Self {
        Self {
            code,
            message,
            data,
        }
    }

    /// Creates a "Parse error" (code -32700).
    ///
    /// This error should be returned when invalid JSON was received by the server.
    ///
    #[cfg(feature = "transit")]
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    /// Creates an "Invalid Request" error (code -32600).
    ///
    /// This error should be returned when the JSON sent is not a valid Request object.
    ///
    #[cfg(feature = "transit")]
    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    /// Creates a "Method not found" error (code -32601).
    ///
    /// This error should be returned when the requested method does not exist
    /// or is not available.
    ///
    pub fn method_not_found() -> Self {
        Self {
            code: -32601, // Method not found
            message: "Method not found".to_string(),
            data: None,
        }
    }

    /// Creates an "Invalid params" error (code -32602) with additional details.
    ///
    /// This error should be returned when the method exists but the provided
    /// parameters are invalid or malformed.
    ///
    /// # Arguments
    ///
    /// * `detail` - Additional information about what was invalid
    ///
    pub fn invalid_params(detail: String) -> Self {
        Self {
            code: -32602, // Invalid params
            message: "Invalid params".to_string(),
            data: Some(detail.into()),
        }
    }

    /// Creates an error for an unknown tool or method variant.
    ///
    /// This is a specialized invalid params error used when a tool
    /// or method variant is not recognized. Uses error code -32602.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the unknown tool
    ///
    pub fn unknown_tool(name: String) -> Self {
        Self {
            code: -32602, // Invalid params
            message: format!("Unknown tool: {}", name),
            data: None,
        }
    }

    /// Creates an "Internal error" (code -32603) from a standard Rust error.
    ///
    /// This error should be used for unexpected internal errors during
    /// method execution. The error's message will be used as the JSON-RPC
    /// error message.
    ///
    /// # Arguments
    ///
    /// * `error` - Any error that implements `std::error::Error`
    ///
    #[cfg(feature = "transit")]
    pub fn from_error<E: std::error::Error>(error: E) -> Self {
        Self {
            code: -32603, // Internal error
            message: error.to_string(),
            data: None,
        }
    }

    /// Creates an "Internal error" (code -32603) with a custom message.
    ///
    /// This is a convenience method for creating internal errors without
    /// needing an actual error object.
    ///
    /// # Arguments
    ///
    /// * `message` - Description of the internal error
    ///
    #[cfg(feature = "transit")]
    pub fn internal_error(message: String) -> Self {
        Self {
            code: -32603,
            message,
            data: None,
        }
    }
}
