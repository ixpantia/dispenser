mod deployment;
mod host_cpu;
mod host_disk;
mod host_memory;
mod output;
mod status;

pub use deployment::DeploymentEvent;
pub use host_cpu::HostCpuEvent;
pub use host_disk::HostDiskEvent;
pub use host_memory::HostMemoryEvent;
pub use output::ContainerOutputEvent;
pub use status::ContainerStatusEvent;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum DispenserEvent {
    Deployment(Box<DeploymentEvent>),
    ContainerStatus(Box<ContainerStatusEvent>),
    LogBatch(ExportLogsServiceRequest),
    SpanBatch(ExportTraceServiceRequest),
    ContainerOutput(ContainerOutputEvent),
    HostCpu(Box<HostCpuEvent>),
    HostMemory(Box<HostMemoryEvent>),
    HostDisk(Box<HostDiskEvent>),
}
