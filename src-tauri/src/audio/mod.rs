mod model;
mod processing;
mod queue;
mod timeline;

#[cfg(windows)]
mod windows;

pub(crate) use model::{
    AudioDevice, AudioDeviceList, AudioDirection, AudioPrototypeConfig, AudioPrototypeStartRequest,
    AudioPrototypeStatus, AudioSource, ChannelStatus, DeviceRole, DeviceSelection,
    LevelDiagnostics, NativeAudioFormat, NativeSampleType, PrototypeRunState,
};
pub(crate) use processing::{ProcessedAudioChunk, ProcessingOutcome, SourceProcessor};
pub(crate) use queue::{
    AudioPacket, BoundedReceiver, BoundedSender, EnqueueResult, PacketReceiver, PacketSender,
    bounded_queue, packet_queue,
};
pub(crate) use timeline::{QpcEpoch, qpc_ticks_to_100ns};

#[cfg(windows)]
pub(crate) use windows::{AudioPrototypeError, AudioPrototypeService};
