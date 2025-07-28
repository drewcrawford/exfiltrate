use std::fmt::{Display, Formatter};
use serde::Serialize;

#[derive(serde::Deserialize,serde::Serialize,Debug,Clone)]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: serde_json::Value,
}

#[derive(serde::Deserialize,serde::Serialize, Debug)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl Notification {
    pub fn new(method: String, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        }
    }
}

#[derive(Debug, serde::Serialize,serde::Deserialize)]
pub struct Response<R> {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    pub id: serde_json::Value,
}

impl<R> Response<R> {
    pub fn new(result: R, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn err(e: Error, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(e),
            id,
        }
    }

    pub fn erase(self) -> Response<serde_json::Value> where R: Serialize {
        Response {
            jsonrpc: self.jsonrpc,
            result: self.result.map(|r| serde_json::to_value(r).unwrap()),
            error: self.error,
            id: self.id,
        }
    }
}


#[derive(Debug, serde::Serialize,serde::Deserialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl Error {
    fn new(code: i32, message: String, data: Option<serde_json::Value>) -> Self {
        Self {
            code,
            message,
            data,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: -32601, // Method not found
            message: "Method not found".to_string(),
            data: None,
        }
    }
    pub fn invalid_params(detail: String) -> Self {
        Self {
            code: -32602, // Invalid params
            message: "Invalid params".to_string(),
            data: Some(detail.into()),
        }
    }

    pub fn unknown_tool(name: String) -> Self {
        Self {
            code: -32602, // Invalid params
            message: format!("Unknown tool: {}", name),
            data: None,
        }
    }

    pub fn from_error<E: std::error::Error>(error: E) -> Self {
        Self {
            code: -32603, // Internal error
            message: error.to_string(),
            data: None,
        }
    }
}