use std::{
    fmt,
    sync::Arc,
};

use crate::types::{
    PlayerState,
    PlayerStateInput,
    QueueState,
    QueueStateInput,
};
use tokio::sync::{
    broadcast,
    mpsc::{
        self,
    },
};

use crate::svc::error::SvcError;

pub mod audio;
pub mod error;
pub mod ext;
pub mod supervisor;
pub mod tracks;

pub type AudioServiceCommands = CommandWithMeta<AudioServiceCommand, AudioServiceResponseTx>;

pub type AudioServiceCommandTx = mpsc::Sender<AudioServiceCommands>;
pub type AudioServiceCommandRx = mpsc::Receiver<AudioServiceCommands>;

pub type AudioServiceResponseRx = mpsc::Receiver<AudioServiceResponse>;
pub type AudioServiceResponseTx = mpsc::Sender<AudioServiceResponse>;

pub type QueueManagerServiceCommands =
    CommandWithMeta<QueueManagerServiceCommand, QueueManagerServiceResponseTx>;

pub type QueueManagerServiceCommandTx = mpsc::Sender<QueueManagerServiceCommands>;
pub type QueueManagerServiceCommandRx = mpsc::Receiver<QueueManagerServiceCommands>;

pub type QueueManagerServiceResponseRx = mpsc::Receiver<QueueManagerServiceResponse>;
pub type QueueManagerServiceResponseTx = mpsc::Sender<QueueManagerServiceResponse>;

pub type ServiceEventsRx<T> = broadcast::Receiver<ServiceEvent<T>>;
pub type ServiceEventsTx<T> = broadcast::Sender<ServiceEvent<T>>;

#[derive(Clone, Debug)]
pub struct CommandWithMeta<I, O>
where
    I: Clone + fmt::Debug,
    O: Clone + fmt::Debug,
{
    pub response_channel: O,
    pub cmd: I,
}

impl<I, O> CommandWithMeta<I, O>
where
    I: Clone + fmt::Debug,
    O: Clone + fmt::Debug,
{
    pub fn new(chan: O, cmd: I) -> Self {
        Self {
            response_channel: chan,
            cmd,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum ServiceEvent<S: Clone + fmt::Debug> {
    StateChange(S),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum QueueManagerServiceCommand {
    ReplaceQueueState(QueueStateInput),
    GetQueueState,
}

#[derive(Debug)]
pub enum QueueManagerServiceResponse {
    QueueState(Arc<QueueState>),
    Error(SvcError),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum AudioServiceCommand {
    Set(PlayerStateInput),
    Get,
}

#[derive(Debug, Default)]
pub enum AudioServiceResponse {
    #[default]
    Unknown,
    Error(SvcError),
    PlayerState(Arc<PlayerState>),
}

pub trait SingletonService {
    fn name(&self) -> String;
    fn run(self) -> impl std::future::Future<Output = ()> + Send;
}

pub trait ServiceFactory {
    type Service: Send;
    fn get_instance(&self) -> impl std::future::Future<Output = Self::Service> + Send;
}
