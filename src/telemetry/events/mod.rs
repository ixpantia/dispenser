mod deployment;
mod output;
mod status;

pub use deployment::DeploymentEvent;
pub use output::ContainerOutputEvent;
pub use status::ContainerStatusEvent;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

#[derive(Debug)]
pub enum DispenserEvent {
    Deployment(Box<DeploymentEvent>),
    ContainerStatus(Box<ContainerStatusEvent>),
    LogBatch(ExportLogsServiceRequest),
    SpanBatch(ExportTraceServiceRequest),
    ContainerOutput(ContainerOutputEvent),
}
