use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};

use super::AudioSource;

#[derive(Debug)]
pub(crate) struct AudioPacket {
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) frames: u32,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnqueueResult {
    Enqueued,
    DroppedFull,
    Disconnected,
}

#[derive(Clone)]
pub(crate) struct PacketSender(Sender<AudioPacket>);

pub(crate) struct PacketReceiver(Receiver<AudioPacket>);

pub(crate) fn packet_queue(capacity: usize) -> (PacketSender, PacketReceiver) {
    let (sender, receiver) = bounded(capacity);
    (PacketSender(sender), PacketReceiver(receiver))
}

impl PacketSender {
    pub(crate) fn try_send(&self, packet: AudioPacket) -> EnqueueResult {
        match self.0.try_send(packet) {
            Ok(()) => EnqueueResult::Enqueued,
            Err(TrySendError::Full(_)) => EnqueueResult::DroppedFull,
            Err(TrySendError::Disconnected(_)) => EnqueueResult::Disconnected,
        }
    }
}

impl PacketReceiver {
    pub(crate) fn receiver(&self) -> &Receiver<AudioPacket> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioPacket, EnqueueResult, packet_queue};
    use crate::audio::AudioSource;

    fn packet(start_ms: u64) -> AudioPacket {
        AudioPacket {
            source: AudioSource::Microphone,
            start_ms,
            frames: 1,
            bytes: vec![0; 4],
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
