mod model;
mod queue;
mod timeline;

#[cfg(windows)]
mod windows;

pub(crate) use model::{
    AudioDevice, AudioDeviceList, AudioDirection, AudioPrototypeConfig, AudioPrototypeStartRequest,
    AudioPrototypeStatus, AudioSource, ChannelStatus, DeviceRole, DeviceSelection,
    NativeAudioFormat, NativeSampleType, PrototypeRunState,
};
pub(crate) use queue::{AudioPacket, EnqueueResult, PacketReceiver, PacketSender, packet_queue};
pub(crate) use timeline::{QpcEpoch, qpc_ticks_to_100ns};

#[cfg(windows)]
pub(crate) use windows::{AudioPrototypeError, AudioPrototypeService};
