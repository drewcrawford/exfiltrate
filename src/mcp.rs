use crate::jrpc::{Request, Response};
pub mod tools;
pub(crate) mod latest_tools;

pub fn dispatch_in_target(request: Request) -> Response<serde_json::Value> {
    if request.method == "tools/list" {
        tools::list_process(request).erase()
    }
    else if request.method == "tools/call" {
        tools::call(request).erase()
    }
    else {
        Response::err(super::jrpc::Error::method_not_found(), request.id)
    }
}

