use crate::jrpc::{Request, Response};
use std::collections::HashMap;
pub mod tools;
mod latest_tools;

pub fn dispatch(request: Request) -> Response<serde_json::Value> {
    if request.method == "tools/list" {
        tools::list(request).erase()
    }
    else if request.method == "tools/call" {
        tools::call(request).erase()
    }
    else {
        Response::err(super::jrpc::Error::new(-32601, "Method not found".to_string(), None), request.id)
    }
}

