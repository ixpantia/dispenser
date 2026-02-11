use bollard::models::{ContainerStateStatusEnum, HealthStatusEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerType {
    Startup,
    Cron,
    ImageUpdate,
    ManualReload,
}

impl AsRef<str> for TriggerType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Startup => "startup",
            Self::Cron => "cron",
            Self::ImageUpdate => "image_update",
            Self::ManualReload => "manual_reload",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerState {
    Empty,
    Created,
    Running,
    Paused,
    Restarting,
    Removing,
    Exited,
    Dead,
    // Custom states
    NotFound,
    Unknown,
}

impl AsRef<str> for ContainerState {
    fn as_ref(&self) -> &str {
        match self {
            Self::Empty => "empty",
            Self::Created => "created",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Restarting => "restarting",
            Self::Removing => "removing",
            Self::Exited => "exited",
            Self::Dead => "dead",
            Self::NotFound => "not_found",
            Self::Unknown => "unknown",
        }
    }
}

impl From<ContainerStateStatusEnum> for ContainerState {
    fn from(status: ContainerStateStatusEnum) -> Self {
        match status {
            ContainerStateStatusEnum::EMPTY => Self::Empty,
            ContainerStateStatusEnum::CREATED => Self::Created,
            ContainerStateStatusEnum::RUNNING => Self::Running,
            ContainerStateStatusEnum::PAUSED => Self::Paused,
            ContainerStateStatusEnum::RESTARTING => Self::Restarting,
            ContainerStateStatusEnum::REMOVING => Self::Removing,
            ContainerStateStatusEnum::EXITED => Self::Exited,
            ContainerStateStatusEnum::DEAD => Self::Dead,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Starting,
    None,
}

impl AsRef<str> for HealthStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
            Self::Starting => "starting",
            Self::None => "none",
        }
    }
}

impl From<HealthStatusEnum> for HealthStatus {
    fn from(status: HealthStatusEnum) -> Self {
        match status {
            HealthStatusEnum::HEALTHY => Self::Healthy,
            HealthStatusEnum::UNHEALTHY => Self::Unhealthy,
            HealthStatusEnum::STARTING => Self::Starting,
            HealthStatusEnum::EMPTY | HealthStatusEnum::NONE => Self::None,
        }
    }
}
