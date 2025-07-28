use std::collections::HashMap;
use exfiltrate::tools::{Argument, InputSchema, ToolCallResponse};
use exfiltrate::transit::transit_proxy::TransitProxy;

pub struct MyTool {

}
impl exfiltrate::tools::Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }

    fn description(&self) -> &str {
        "A tool that does something"
    }


    fn call(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolCallResponse, exfiltrate::tools::ToolCallError> {
        let params = params.get("input")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| exfiltrate::tools::ToolCallError::new(vec!["Invalid input parameter".into()]))?;
        Ok(ToolCallResponse::new(vec![format!("This is a response from my tool: {}", params).into()]))
    }
    fn input_schema(&self) -> exfiltrate::mcp::tools::InputSchema {
        InputSchema::new(vec![Argument::new("input".to_string(), "number".to_string(), "Your favorite number".to_string(), true)])
    }
}

pub struct EventualTool {

}
impl exfiltrate::tools::Tool for EventualTool {
    fn name(&self) -> &str {
        "eventual_tool"
    }

    fn description(&self) -> &str {
        "A tool that arrives late"
    }


    fn call(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolCallResponse, exfiltrate::tools::ToolCallError> {
        let params = params.get("input")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| exfiltrate::tools::ToolCallError::new(vec!["Invalid input parameter".into()]))?;
        Ok(ToolCallResponse::new(vec![format!("This is a response from my tool: {}", params).into()]))
    }
    fn input_schema(&self) -> exfiltrate::mcp::tools::InputSchema {
        InputSchema::new(vec![Argument::new("input".to_string(), "number".to_string(), "Your favorite number".to_string(), true)])
    }
}

fn main() {
    let proxy = exfiltrate::transit::transit_proxy::TransitProxy::new();
    let server = exfiltrate::transit::http::Server::new("127.0.0.1:1984",proxy);
    exfiltrate::tools::add_tool(Box::new(MyTool{}));
    std::thread::sleep(std::time::Duration::from_secs(10));
    //insert a new tool
    exfiltrate::tools::add_tool(Box::new(EventualTool {}));
    println!("Added late tool");
    std::thread::sleep(std::time::Duration::from_secs(1000));
}