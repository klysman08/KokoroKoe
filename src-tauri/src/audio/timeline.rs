#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QpcEpoch {
    epoch_100ns: u64,
}

impl QpcEpoch {
    pub(crate) fn from_100ns(epoch_100ns: u64) -> Self {
        Self { epoch_100ns }
    }

    pub(crate) fn packet_ms(self, packet_qpc_100ns: u64) -> u64 {
        packet_qpc_100ns.saturating_sub(self.epoch_100ns) / 10_000
    }
}

pub(crate) fn qpc_ticks_to_100ns(ticks: i64, frequency: i64) -> Option<u64> {
    if ticks < 0 || frequency <= 0 {
        return None;
    }

    let scaled = (ticks as u128).checked_mul(10_000_000)? / frequency as u128;
    u64::try_from(scaled).ok()
}

#[cfg(test)]
mod tests {
    use super::{QpcEpoch, qpc_ticks_to_100ns};

    const TWO_HOURS_SECONDS: u64 = 2 * 60 * 60;
    const TEST_QPC_FREQUENCY: i64 = 10_000_003;
    const TEST_EPOCH_TICKS: i64 = 4_000_000_000_000_000;

    #[test]
    fn converts_qpc_ticks_without_integer_overflow() {
        assert_eq!(qpc_ticks_to_100ns(30_000_000, 10_000_000), Some(30_000_000));
        assert_eq!(qpc_ticks_to_100ns(i64::MAX, i64::MAX), Some(10_000_000));
        assert_eq!(qpc_ticks_to_100ns(-1, 10_000_000), None);
        assert_eq!(qpc_ticks_to_100ns(1, 0), None);
    }

    #[test]
    fn maps_both_sources_to_one_saturating_session_epoch() {
        let epoch = QpcEpoch::from_100ns(50_000_000);
        assert_eq!(epoch.packet_ms(50_120_000), 12);
        assert_eq!(epoch.packet_ms(50_085_000), 8);
        assert_eq!(epoch.packet_ms(49_999_999), 0);
    }

    #[test]
    fn two_hour_dual_source_alignment_survives_source_local_recovery_gap() {
        let epoch_100ns = qpc_ticks_to_100ns(TEST_EPOCH_TICKS, TEST_QPC_FREQUENCY)
            .expect("large QPC epoch should convert");
        let epoch = QpcEpoch::from_100ns(epoch_100ns);
        let microphone = simulate_source(epoch, 44_100, 441, Some((3_600, 250)));
        let system_output = simulate_source(epoch, 48_000, 512, None);

        assert!(microphone.max_error_ms < 20);
        assert!(system_output.max_error_ms < 20);
        assert_eq!(microphone.timestamp_regressions, 0);
        assert_eq!(system_output.timestamp_regressions, 0);
        assert!(microphone.recovery_jump_ms >= 250);
        assert_eq!(system_output.recovery_jump_ms, 0);
        eprintln!(
            "P3-007 clock gate: duration_s={TWO_HOURS_SECONDS} frequency_hz={TEST_QPC_FREQUENCY} microphone_packets={} system_packets={} microphone_max_error_ms={} system_max_error_ms={} recovery_jump_ms={}",
            microphone.packets,
            system_output.packets,
            microphone.max_error_ms,
            system_output.max_error_ms,
            microphone.recovery_jump_ms,
        );
    }

    #[derive(Debug)]
    struct SimulationResult {
        packets: u64,
        max_error_ms: u64,
        timestamp_regressions: u64,
        recovery_jump_ms: u64,
    }

    fn simulate_source(
        epoch: QpcEpoch,
        sample_rate: u64,
        frames_per_packet: u64,
        recovery: Option<(u64, u64)>,
    ) -> SimulationResult {
        let packet_count = TWO_HOURS_SECONDS * sample_rate / frames_per_packet;
        let recovery_packet =
            recovery.map(|(at_seconds, _)| at_seconds * sample_rate / frames_per_packet);
        let recovery_gap_ms = recovery.map_or(0, |(_, gap_ms)| gap_ms);
        let mut previous = None;
        let mut max_error_ms = 0;
        let mut timestamp_regressions = 0;
        let mut recovery_jump_ms = 0;

        for packet_index in 0..=packet_count {
            let media_100ns = (packet_index as u128 * frames_per_packet as u128 * 10_000_000)
                / sample_rate as u128;
            let gap_100ns = if recovery_packet.is_some_and(|at| packet_index >= at) {
                recovery_gap_ms as u128 * 10_000
            } else {
                0
            };
            let elapsed_100ns = media_100ns + gap_100ns;
            let elapsed_ticks = elapsed_100ns * TEST_QPC_FREQUENCY as u128 / 10_000_000;
            let ticks = TEST_EPOCH_TICKS
                .checked_add(i64::try_from(elapsed_ticks).expect("two-hour ticks fit i64"))
                .expect("absolute QPC ticks fit i64");
            let packet_100ns =
                qpc_ticks_to_100ns(ticks, TEST_QPC_FREQUENCY).expect("packet QPC should convert");
            let actual_ms = epoch.packet_ms(packet_100ns);
            let expected_ms =
                u64::try_from(elapsed_100ns / 10_000).expect("two-hour expected time fits u64");
            max_error_ms = max_error_ms.max(actual_ms.abs_diff(expected_ms));

            if let Some(previous_ms) = previous {
                timestamp_regressions += u64::from(actual_ms < previous_ms);
                if recovery_packet == Some(packet_index) {
                    recovery_jump_ms = actual_ms.saturating_sub(previous_ms);
                    assert!(
                        actual_ms >= expected_ms.saturating_sub(1),
                        "the shared epoch must not rebase or compress the recovery gap"
                    );
                }
            }
            previous = Some(actual_ms);
        }

        SimulationResult {
            packets: packet_count + 1,
            max_error_ms,
            timestamp_regressions,
            recovery_jump_ms,
        }
    }
}
