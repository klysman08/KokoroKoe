use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};

use super::{AudioSource, NativeAudioFormat};

#[derive(Debug)]
pub(crate) struct AudioPacket {
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) frames: u32,
    pub(crate) bytes: Vec<u8>,
    pub(crate) format: NativeAudioFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnqueueResult {
    Enqueued,
    DroppedFull,
    Disconnected,
}

pub(crate) struct BoundedSender<T>(Sender<T>);

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub(crate) struct BoundedReceiver<T>(Receiver<T>);

pub(crate) type PacketSender = BoundedSender<AudioPacket>;
pub(crate) type PacketReceiver = BoundedReceiver<AudioPacket>;

pub(crate) fn packet_queue(capacity: usize) -> (PacketSender, PacketReceiver) {
    bounded_queue(capacity)
}

pub(crate) fn bounded_queue<T>(capacity: usize) -> (BoundedSender<T>, BoundedReceiver<T>) {
    let (sender, receiver) = bounded(capacity);
    (BoundedSender(sender), BoundedReceiver(receiver))
}

impl<T> BoundedSender<T> {
    pub(crate) fn try_send(&self, value: T) -> EnqueueResult {
        match self.0.try_send(value) {
            Ok(()) => EnqueueResult::Enqueued,
            Err(TrySendError::Full(_)) => EnqueueResult::DroppedFull,
            Err(TrySendError::Disconnected(_)) => EnqueueResult::Disconnected,
        }
    }

    pub(crate) fn send(&self, value: T) -> Result<(), T> {
        self.0.send(value).map_err(|error| error.0)
    }

    #[cfg(test)]
    pub(crate) fn queued_len(&self) -> usize {
        self.0.len()
    }
}

impl<T> BoundedReceiver<T> {
    pub(crate) fn receiver(&self) -> &Receiver<T> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioPacket, EnqueueResult, packet_queue};
    use crate::audio::{AudioSource, NativeAudioFormat, NativeSampleType};

    fn packet(start_ms: u64) -> AudioPacket {
        AudioPacket {
            source: AudioSource::Microphone,
            start_ms,
            frames: 1,
            bytes: vec![0; 4],
            format: NativeAudioFormat {
                sample_rate: 16_000,
                channels: 1,
                bits_per_sample: 32,
                valid_bits_per_sample: 32,
                block_align: 4,
                channel_mask: 4,
                sample_type: NativeSampleType::Float,
            },
        }
    }

    #[test]
    fn bounded_queue_drops_new_packets_instead_of_growing() {
        let (sender, receiver) = packet_queue(2);
        assert_eq!(sender.try_send(packet(1)), EnqueueResult::Enqueued);
        assert_eq!(sender.try_send(packet(2)), EnqueueResult::Enqueued);
        assert_eq!(sender.try_send(packet(3)), EnqueueResult::DroppedFull);

        assert_eq!(receiver.receiver().len(), 2);
        assert_eq!(receiver.receiver().recv().unwrap().start_ms, 1);
        assert_eq!(receiver.receiver().recv().unwrap().start_ms, 2);
    }

    #[test]
    fn independent_source_queues_do_not_starve_each_other() {
        let (microphone_sender, _microphone_receiver) = packet_queue(1);
        let (system_sender, system_receiver) = packet_queue(1);

        assert_eq!(
            microphone_sender.try_send(packet(1)),
            EnqueueResult::Enqueued
        );
        assert_eq!(
            microphone_sender.try_send(packet(2)),
            EnqueueResult::DroppedFull
        );
        assert_eq!(system_sender.try_send(packet(3)), EnqueueResult::Enqueued);
        assert_eq!(system_receiver.receiver().recv().unwrap().start_ms, 3);
    }
}
