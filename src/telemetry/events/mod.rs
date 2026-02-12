mod deployment;
mod output;
mod status;

pub use deployment::DeploymentEvent;
pub use output::ContainerOutputEvent;
pub use status::ContainerStatusEvent;

use super::otlp::{LogsData, TracesData};

#[derive(Debug)]
pub enum DispenserEvent {
    Deployment(Box<DeploymentEvent>),
    ContainerStatus(Box<ContainerStatusEvent>),
    LogBatch(LogsData),
    SpanBatch(TracesData),
    ContainerOutput(ContainerOutputEvent),
}
