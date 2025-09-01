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
//! # Examples
//!
//! ## Creating and handling a request
//!
//! ```
//! use exfiltrate::jrpc::{Request, Response, Error};
//! use serde_json::json;
//!
//! // Create a request
//! let request = Request {
//!     jsonrpc: "2.0".to_string(),
//!     method: "add".to_string(),
//!     params: Some(json!([2, 3])),
//!     id: json!(1),
//! };
//!
//! // Serialize for transmission
//! let serialized = serde_json::to_string(&request).unwrap();
//! assert!(serialized.contains("\"method\":\"add\""));
//!
//! // Process the request and create a response
//! let result = 5; // Actual processing would happen here
//! let response = Response::new(json!(result), request.id.clone());
//! assert_eq!(response.result, Some(json!(5)));
//!
//! // Or return an error
//! let error_response: Response<serde_json::Value> = Response::err(
//!     Error::invalid_params("Expected two numbers".to_string()),
//!     request.id
//! );
//! assert!(error_response.error.is_some());
//! ```
//!
//! ## Sending notifications
//!
//! ```
//! use exfiltrate::jrpc::Notification;
//! use serde_json::json;
//!
//! // Create a notification (no response expected)
//! let notification = Notification::new(
//!     "log".to_string(),
//!     Some(json!({"level": "info", "message": "System started"}))
//! );
//!
//! // Notifications can be serialized and sent without expecting a response
//! let serialized = serde_json::to_string(&notification).unwrap();
//! assert!(serialized.contains("\"method\":\"log\""));
//! assert!(!serialized.contains("\"id\"")); // No id field in notifications
//! ```
//!
//! ## Batch operations
//!
//! ```
//! use exfiltrate::jrpc::{Request, Notification};
//! use serde_json::json;
//!
//! // JSON-RPC 2.0 supports batch operations by sending arrays
//! let batch = vec![
//!     json!(Request {
//!         jsonrpc: "2.0".to_string(),
//!         method: "get_user".to_string(),
//!         params: Some(json!({"id": 123})),
//!         id: json!(1),
//!     }),
//!     json!(Notification::new(
//!         "log".to_string(),
//!         Some(json!({"action": "user_query"}))
//!     )),
//! ];
//!
//! let batch_json = serde_json::to_string(&batch).unwrap();
//! assert!(batch_json.starts_with('['));
//! ```
//!
//! ## Error handling patterns
//!
//! ```
//! use exfiltrate::jrpc::{Request, Response, Error};
//! use serde_json::json;
//!
//! fn handle_request(request: Request) -> Response<serde_json::Value> {
//!     match request.method.as_str() {
//!         "echo" => {
//!             // Echo back the params
//!             Response::new(request.params.unwrap_or(json!(null)), request.id)
//!         }
//!         "divide" => {
//!             // Validate params and handle errors
//!             let params = match request.params {
//!                 Some(p) => p,
//!                 None => return Response::err(
//!                     Error::invalid_params("Missing parameters".to_string()),
//!                     request.id
//!                 ),
//!             };
//!             
//!             // Further processing...
//!             Response::new(json!({"result": "calculated"}), request.id)
//!         }
//!         _ => {
//!             // Unknown method
//!             Response::err(Error::method_not_found(), request.id)
//!         }
//!     }
//! }
//!
//! let request = Request {
//!     jsonrpc: "2.0".to_string(),
//!     method: "unknown".to_string(),
//!     params: None,
//!     id: json!(42),
//! };
//!
//! let response = handle_request(request);
//! assert!(response.error.is_some());
//! assert_eq!(response.error.unwrap().code, -32601);
//! ```

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
/// # Examples
///
/// ## Basic request without parameters
///
/// ```
/// use exfiltrate::jrpc::Request;
/// use serde_json::json;
///
/// let request = Request {
///     jsonrpc: "2.0".to_string(),
///     method: "tools/list".to_string(),
///     params: None,
///     id: json!("unique-id-123"),
/// };
///
/// // Serialize to JSON for transmission
/// let json_str = serde_json::to_string(&request).unwrap();
/// assert!(json_str.contains("\"jsonrpc\":\"2.0\""));
/// ```
///
/// ## Request with array parameters
///
/// ```
/// use exfiltrate::jrpc::Request;
/// use serde_json::json;
///
/// let request = Request {
///     jsonrpc: "2.0".to_string(),
///     method: "subtract".to_string(),
///     params: Some(json!([42, 23])),
///     id: json!(1),
/// };
///
/// assert_eq!(request.params, Some(json!([42, 23])));
/// ```
///
/// ## Request with object parameters
///
/// ```
/// use exfiltrate::jrpc::Request;
/// use serde_json::json;
///
/// let request = Request {
///     jsonrpc: "2.0".to_string(),
///     method: "user/update".to_string(),
///     params: Some(json!({
///         "id": 123,
///         "name": "Alice",
///         "email": "alice@example.com"
///     })),
///     id: json!("req-456"),
/// };
///
/// // Deserialize from JSON
/// let json_str = r#"{
///     "jsonrpc": "2.0",
///     "method": "ping",
///     "id": 99
/// }"#;
/// let deserialized: Request = serde_json::from_str(json_str).unwrap();
/// assert_eq!(deserialized.method, "ping");
/// assert_eq!(deserialized.id, json!(99));
/// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Request;
    /// use serde_json::json;
    ///
    /// // Simple request without parameters
    /// let request = Request::new(
    ///     "ping".to_string(),
    ///     None,
    ///     json!(1)
    /// );
    /// assert_eq!(request.jsonrpc, "2.0");
    /// assert_eq!(request.method, "ping");
    ///
    /// // Request with parameters
    /// let request_with_params = Request::new(
    ///     "calculate".to_string(),
    ///     Some(json!({"x": 10, "y": 20})),
    ///     json!("calc-123")
    /// );
    /// assert!(request_with_params.params.is_some());
    /// ```
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
/// # Examples
///
/// ```
/// use exfiltrate::jrpc::Notification;
/// use serde_json::json;
///
/// // Create a notification for logging
/// let log_notification = Notification::new(
///     "log".to_string(),
///     Some(json!({"level": "error", "message": "Connection failed"}))
/// );
///
/// // Create a notification without parameters
/// let heartbeat = Notification::new("heartbeat".to_string(), None);
/// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Notification;
    /// use serde_json::json;
    ///
    /// let notification = Notification::new(
    ///     "status_update".to_string(),
    ///     Some(json!({"status": "running", "progress": 50}))
    /// );
    ///
    /// assert_eq!(notification.jsonrpc, "2.0");
    /// assert_eq!(notification.method, "status_update");
    /// ```
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
/// # Examples
///
/// ## Creating successful responses
///
/// ```
/// use exfiltrate::jrpc::{Response, Error};
/// use serde_json::json;
///
/// // Create a successful response with JSON value
/// let success_response = Response::new(json!({"status": "ok"}), json!(1));
/// assert!(success_response.result.is_some());
/// assert!(success_response.error.is_none());
///
/// // Create a typed response
/// let typed_response: Response<i32> = Response::new(42, json!("req-123"));
/// assert_eq!(typed_response.result, Some(42));
/// ```
///
/// ## Creating error responses
///
/// ```
/// use exfiltrate::jrpc::{Response, Error};
/// use serde_json::json;
///
/// // Create an error response
/// let error_response: Response<serde_json::Value> = Response::err(
///     Error::method_not_found(),
///     json!(2)
/// );
/// assert!(error_response.result.is_none());
/// assert!(error_response.error.is_some());
/// assert_eq!(error_response.error.as_ref().unwrap().code, -32601);
/// ```
///
/// ## Serialization and deserialization
///
/// ```
/// use exfiltrate::jrpc::Response;
/// use serde_json::json;
///
/// let response = Response::new(json!(["item1", "item2"]), json!(99));
///
/// // Serialize to JSON
/// let json_str = serde_json::to_string(&response).unwrap();
/// assert!(json_str.contains("\"result\""));
/// assert!(!json_str.contains("\"error\"")); // error field is omitted when None
///
/// // Deserialize from JSON
/// let json_str = r#"{
///     "jsonrpc": "2.0",
///     "result": 19,
///     "id": 1
/// }"#;
/// let deserialized: Response<i32> = serde_json::from_str(json_str).unwrap();
/// assert_eq!(deserialized.result, Some(19));
/// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Response;
    /// use serde_json::json;
    ///
    /// let response = Response::new(42, json!("request-123"));
    /// assert!(response.result.is_some());
    /// assert!(response.error.is_none());
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::{Response, Error};
    /// use serde_json::json;
    ///
    /// let response: Response<String> = Response::err(
    ///     Error::invalid_params("Missing required field".to_string()),
    ///     json!(123)
    /// );
    /// assert!(response.result.is_none());
    /// assert!(response.error.is_some());
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::{Response, Error};
    /// use serde_json::json;
    ///
    /// #[derive(serde::Serialize)]
    /// struct CustomResult {
    ///     value: i32,
    ///     message: String,
    /// }
    ///
    /// // Type erasure for successful response
    /// let typed_response = Response::new(
    ///     CustomResult { value: 42, message: "Success".to_string() },
    ///     json!(1)
    /// );
    /// let erased_response = typed_response.erase();
    /// // erased_response is now Response<serde_json::Value>
    /// assert_eq!(
    ///     erased_response.result.as_ref().unwrap()["value"],
    ///     json!(42)
    /// );
    ///
    /// // Type erasure preserves error responses
    /// let error_response: Response<CustomResult> = Response::err(
    ///     Error::invalid_params("Bad input".to_string()),
    ///     json!(2)
    /// );
    /// let erased_error = error_response.erase();
    /// assert!(erased_error.error.is_some());
    /// assert_eq!(erased_error.error.as_ref().unwrap().code, -32602);
    /// ```
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
/// # Examples
///
/// ```
/// use exfiltrate::jrpc::Error;
///
/// // Create a standard error
/// let error = Error::method_not_found();
/// assert_eq!(error.code, -32601);
///
/// // Create an error with additional details
/// let detailed_error = Error::invalid_params("Expected an array".to_string());
/// assert_eq!(detailed_error.code, -32602);
/// assert!(detailed_error.data.is_some());
/// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    /// use serde_json::json;
    ///
    /// let custom_error = Error::new(
    ///     -32050,
    ///     "Database connection failed".to_string(),
    ///     Some(json!({"retry_after": 5}))
    /// );
    /// assert_eq!(custom_error.code, -32050);
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    ///
    /// let error = Error::parse_error();
    /// assert_eq!(error.code, -32700);
    /// assert_eq!(error.message, "Parse error");
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    ///
    /// let error = Error::invalid_request();
    /// assert_eq!(error.code, -32600);
    /// assert_eq!(error.message, "Invalid Request");
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::{Request, Response, Error};
    /// use serde_json::json;
    ///
    /// let request = Request::new(
    ///     "non_existent_method".to_string(),
    ///     None,
    ///     json!(1)
    /// );
    ///
    /// // Return method not found error
    /// let response: Response<serde_json::Value> = Response::err(
    ///     Error::method_not_found(),
    ///     request.id
    /// );
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    ///
    /// let error = Error::invalid_params(
    ///     "Expected 2 parameters, got 3".to_string()
    /// );
    /// assert_eq!(error.code, -32602);
    /// assert_eq!(error.data, Some(serde_json::Value::String(
    ///     "Expected 2 parameters, got 3".to_string()
    /// )));
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    ///
    /// let error = Error::unknown_tool("undefined_tool".to_string());
    /// assert_eq!(error.message, "Unknown tool: undefined_tool");
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    /// use std::io;
    ///
    /// let io_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
    /// let jrpc_error = Error::from_error(io_error);
    /// assert_eq!(jrpc_error.code, -32603);
    /// assert_eq!(jrpc_error.message, "File not found");
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::jrpc::Error;
    ///
    /// let error = Error::internal_error("Database connection lost".to_string());
    /// assert_eq!(error.code, -32603);
    /// assert_eq!(error.message, "Database connection lost");
    /// ```
    pub fn internal_error(message: String) -> Self {
        Self {
            code: -32603,
            message,
            data: None,
        }
    }
}
