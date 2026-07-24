use tokio::sync::{
    broadcast,
    mpsc,
};

use crate::svc::CommandWithMeta;

pub mod cover_img;

/// Command wrapper for external service communication.
pub type ExternalServiceCommands = CommandWithMeta<ExtServiceCommand, ExternalServiceResponseTx>;

pub type ExternalServiceCommandTx = broadcast::Sender<ExternalServiceCommands>;

pub type ExternalServiceCommandRx = broadcast::Receiver<ExternalServiceCommands>;

pub type ExternalServiceResponseRx = mpsc::Receiver<ExtServiceResponse>;

pub type ExternalServiceResponseTx = mpsc::Sender<ExtServiceResponse>;

/// Commands that can be sent to external services. (Input)
#[derive(Debug, Clone)]
pub enum ExtServiceCommand {
    /// Binary data payload. Can be anything binary.
    Data(Vec<u8>),
    RegisterClient,
    UnregisterClient,
}

/// Responses received from external services. (Output)
#[derive(Debug, Clone)]
pub enum ExtServiceResponse {
    /// Any binary data coming out of the external service.
    Data(Vec<u8>),
    Ok,
    Error,
}
