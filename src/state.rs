use crate::{
    config::{
        Server,
        Subsonic,
    },
    svc::supervisor::ServiceSupervisor,
};

/// Application state (unused for now)
pub struct ResubnanceState {
    pub server_conf: Server,
    pub audio_conf: Subsonic,
    pub supervisor: ServiceSupervisor,
}
