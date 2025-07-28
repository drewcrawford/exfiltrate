pub enum SendMessage {
    Request(crate::jrpc::Request),
    Notification(crate::jrpc::Notification),
}

