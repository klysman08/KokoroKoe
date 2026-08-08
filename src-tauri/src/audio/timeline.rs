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
}
