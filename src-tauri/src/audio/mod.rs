mod model;
mod processing;
mod queue;
mod timeline;
mod vad;

#[cfg(windows)]
mod windows;

pub(crate) use model::{
    AudioDevice, AudioDeviceList, AudioDeviceState, AudioDeviceStatusChanged, AudioDirection,
    AudioEventEnvelope, AudioLevelUpdated, AudioPrototypeConfig, AudioPrototypeStatus, AudioSource,
    ChannelDiagnostics, ChannelHealth, ChannelHealthStatus, ChannelStatus, DeviceRole,
    DeviceSelection, DeviceTestInput, DeviceTestRunStatus, DeviceTestStatus, LevelDiagnostics,
    NativeAudioFormat, NativeSampleType, PrototypeRunState, UtteranceDiagnostics,
};
pub(crate) use processing::{
    FinalizedAudioUpdate, ProcessedAudioChunk, ProcessingOutcome, SourceProcessor,
};
pub(crate) use queue::{
    AudioPacket, BoundedReceiver, BoundedSender, EnqueueResult, PacketReceiver, PacketSender,
    bounded_queue, packet_queue,
};
pub(crate) use timeline::{QpcEpoch, qpc_ticks_to_100ns};
pub(crate) use vad::{
    DetectedUtterance, PartialUtteranceSnapshot, UtteranceEndReason, VadProcessOutcome,
    VadSegmenter,
};

#[cfg(windows)]
pub(crate) use windows::{AudioDeviceTestObservation, AudioDeviceTestService, AudioPrototypeError};

#[cfg(windows)]
pub(crate) use windows::RunningAudioPrototype;
