use tokio::task::JoinHandle;
use tracing::{
    Instrument,
    Level,
};

use crate::svc::{
    SingletonService,
    error::SvcError,
    ext::{
        ExternalServiceCommandRx,
        ExternalServiceCommandTx,
    },
};

pub struct ServiceSupervisor {
    service_handles: Vec<JoinHandle<()>>,
    external_broadcast_channel: ExternalServiceCommandTx,
}

impl ServiceSupervisor {
    pub fn new(external_broadcast_channel: ExternalServiceCommandTx) -> Self {
        Self {
            service_handles: vec![],
            external_broadcast_channel,
        }
    }

    pub fn external_tx(&self) -> ExternalServiceCommandTx {
        self.external_broadcast_channel.clone()
    }

    pub fn external_rx(&self) -> ExternalServiceCommandRx {
        self.external_broadcast_channel.subscribe()
    }

    pub async fn start<S: SingletonService + Send + Sync + 'static>(&mut self, svc: S) {
        self.service_handles.push(tokio::task::spawn(async move {
            let name = svc.name();
            let span = tracing::span!(Level::DEBUG, "supervisor::run", service = name);
            svc.run().instrument(span).await;
            tracing::warn!(service = name, "service exited. quitting program.");
            // these are long running tasks, if they exit the program is done.
            // std::process::exit(SvcError::Internal().into());
        }));
    }
}
