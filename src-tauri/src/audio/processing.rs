use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters,
    audioadapter_buffers::direct::InterleavedSlice,
};

use super::{
    AudioPacket, AudioSource, DetectedUtterance, LevelDiagnostics, NativeAudioFormat,
    NativeSampleType, VadProcessOutcome, VadSegmenter,
};

pub(crate) const TARGET_SAMPLE_RATE: u32 = 16_000;
const NORMALIZED_CHUNK_SAMPLES: usize = 160;
const LEVEL_WINDOW_SAMPLES: usize = 1_600;
const LEVEL_FLOOR_DBFS: f64 = -120.0;
const CLIPPING_THRESHOLD: f32 = 0.999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessingError {
    pub(crate) code: &'static str,
}

impl ProcessingError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

#[derive(Debug)]
pub(crate) struct ProcessedAudioChunk {
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) samples: Vec<f32>,
}

#[derive(Debug)]
pub(crate) struct ProcessingOutcome {
    pub(crate) chunks: Vec<ProcessedAudioChunk>,
    pub(crate) native_frames_decoded: u64,
    pub(crate) non_finite_samples_sanitized: u64,
    pub(crate) format_changed: bool,
    pub(crate) resampler_delay_frames: u64,
    pub(crate) pending_native_frames: u64,
    pub(crate) pending_normalized_samples: u64,
    pub(crate) level_updates: Vec<LevelDiagnostics>,
    pub(crate) utterances: Vec<DetectedUtterance>,
    pub(crate) vad_frames_analyzed: u64,
    pub(crate) vad_speech_frames: u64,
    pub(crate) vad_silence_frames: u64,
    pub(crate) short_utterances_rejected: u64,
    pub(crate) forced_splits: u64,
    pub(crate) vad_resets: u64,
    pub(crate) vad_pending_samples: u64,
    pub(crate) vad_buffered_samples: u64,
}

impl ProcessingOutcome {
    fn merge_vad(&mut self, outcome: VadProcessOutcome) {
        self.utterances.extend(outcome.utterances);
        self.vad_frames_analyzed = self
            .vad_frames_analyzed
            .saturating_add(outcome.frames_analyzed);
        self.vad_speech_frames = self.vad_speech_frames.saturating_add(outcome.speech_frames);
        self.vad_silence_frames = self
            .vad_silence_frames
            .saturating_add(outcome.silence_frames);
        self.short_utterances_rejected = self
            .short_utterances_rejected
            .saturating_add(outcome.short_utterances_rejected);
        self.forced_splits = self.forced_splits.saturating_add(outcome.forced_splits);
        self.vad_resets = self.vad_resets.saturating_add(outcome.resets);
        self.vad_pending_samples = outcome.pending_samples;
        self.vad_buffered_samples = outcome.buffered_samples;
    }

    fn vad_only(outcome: VadProcessOutcome) -> Self {
        let mut processing = Self {
            chunks: Vec::new(),
            native_frames_decoded: 0,
            non_finite_samples_sanitized: 0,
            format_changed: false,
            resampler_delay_frames: 0,
            pending_native_frames: 0,
            pending_normalized_samples: 0,
            level_updates: Vec::new(),
            utterances: Vec::new(),
            vad_frames_analyzed: 0,
            vad_speech_frames: 0,
            vad_silence_frames: 0,
            short_utterances_rejected: 0,
            forced_splits: 0,
            vad_resets: 0,
            vad_pending_samples: 0,
            vad_buffered_samples: 0,
        };
        processing.merge_vad(outcome);
        processing
    }
}

pub(crate) struct SourceProcessor {
    source: AudioSource,
    pipeline: Option<FormatPipeline>,
    vad: VadSegmenter,
}

impl SourceProcessor {
    pub(crate) fn new(source: AudioSource) -> Self {
        Self {
            source,
            pipeline: None,
            vad: VadSegmenter::new(source),
        }
    }

    pub(crate) fn process(
        &mut self,
        packet: AudioPacket,
    ) -> Result<ProcessingOutcome, ProcessingError> {
        if packet.source != self.source {
            return Err(ProcessingError::new("audio_processing_source_mismatch"));
        }

        let format_changed = self
            .pipeline
            .as_ref()
            .is_some_and(|pipeline| pipeline.format != packet.format);
        let reset_outcome = format_changed.then(|| self.vad.format_changed());
        if self
            .pipeline
            .as_ref()
            .is_none_or(|pipeline| pipeline.format != packet.format)
        {
            self.pipeline = Some(FormatPipeline::new(
                packet.format.clone(),
                packet.start_ms,
                self.source,
            )?);
        }

        let (mono, mut sanitized) = decode_and_downmix(&packet)?;
        let pipeline = self
            .pipeline
            .as_mut()
            .expect("processing pipeline initialized above");
        let processed = pipeline.push(&mono)?;
        sanitized = sanitized.saturating_add(processed.non_finite_samples_sanitized);

        let mut outcome = ProcessingOutcome {
            chunks: processed.chunks,
            native_frames_decoded: packet.frames as u64,
            non_finite_samples_sanitized: sanitized,
            format_changed,
            resampler_delay_frames: pipeline.resampler_delay_frames,
            pending_native_frames: pipeline.native_buffer.len() as u64,
            pending_normalized_samples: pipeline.normalized_buffer.len() as u64,
            level_updates: processed.level_updates,
            utterances: Vec::new(),
            vad_frames_analyzed: 0,
            vad_speech_frames: 0,
            vad_silence_frames: 0,
            short_utterances_rejected: 0,
            forced_splits: 0,
            vad_resets: 0,
            vad_pending_samples: 0,
            vad_buffered_samples: 0,
        };
        if let Some(reset) = reset_outcome {
            outcome.merge_vad(reset);
        }
        let vad_outcomes: Vec<_> = outcome
            .chunks
            .iter()
            .map(|chunk| self.vad.push(chunk.source, chunk.start_ms, &chunk.samples))
            .collect();
        if vad_outcomes.is_empty() {
            outcome.merge_vad(self.vad.snapshot());
        }
        for vad_outcome in vad_outcomes {
            outcome.merge_vad(vad_outcome);
        }
        Ok(outcome)
    }

    pub(crate) fn finish(&mut self) -> ProcessingOutcome {
        let mut outcome = ProcessingOutcome::vad_only(self.vad.finish());
        if let Some(pipeline) = self.pipeline.as_ref() {
            outcome.resampler_delay_frames = pipeline.resampler_delay_frames;
            outcome.pending_native_frames = pipeline.native_buffer.len() as u64;
            outcome.pending_normalized_samples = pipeline.normalized_buffer.len() as u64;
        }
        outcome
    }
}

struct FormatPipeline {
    source: AudioSource,
    format: NativeAudioFormat,
    resampler: Option<Async<f32>>,
    native_buffer: Vec<f32>,
    resample_output: Vec<f32>,
    normalized_buffer: Vec<f32>,
    timeline_origin_ms: u64,
    normalized_chunks_emitted: u64,
    resampler_delay_frames: u64,
    meter: LevelMeter,
}

impl FormatPipeline {
    fn new(
        format: NativeAudioFormat,
        timeline_origin_ms: u64,
        source: AudioSource,
    ) -> Result<Self, ProcessingError> {
        validate_format(&format)?;
        let resampler = if format.sample_rate == TARGET_SAMPLE_RATE {
            None
        } else {
            let chunk_size = usize::try_from(format.sample_rate.div_ceil(100))
                .map_err(|_| ProcessingError::new("audio_processing_format_invalid"))?;
            Some(
                Async::<f32>::new_sinc(
                    TARGET_SAMPLE_RATE as f64 / format.sample_rate as f64,
                    1.0,
                    &SincInterpolationParameters::default(),
                    chunk_size,
                    1,
                    FixedAsync::Input,
                )
                .map_err(|_| ProcessingError::new("audio_resampler_unavailable"))?,
            )
        };
        let resampler_delay_frames = resampler
            .as_ref()
            .map_or(0, |resampler| resampler.output_delay() as u64);
        let output_capacity = resampler
            .as_ref()
            .map_or(NORMALIZED_CHUNK_SAMPLES, Resampler::output_frames_max);

        Ok(Self {
            source,
            format,
            resampler,
            native_buffer: Vec::new(),
            resample_output: vec![0.0; output_capacity],
            normalized_buffer: Vec::new(),
            timeline_origin_ms,
            normalized_chunks_emitted: 0,
            resampler_delay_frames,
            meter: LevelMeter::default(),
        })
    }

    fn push(&mut self, mono: &[f32]) -> Result<PipelineOutput, ProcessingError> {
        let mut sanitized = 0_u64;
        if self.resampler.is_none() {
            self.normalized_buffer.extend_from_slice(mono);
        } else {
            self.native_buffer.extend_from_slice(mono);
            loop {
                let input_frames = self
                    .resampler
                    .as_ref()
                    .expect("checked above")
                    .input_frames_next();
                if self.native_buffer.len() < input_frames {
                    break;
                }
                let output_frames = self
                    .resampler
                    .as_ref()
                    .expect("checked above")
                    .output_frames_max();
                self.resample_output.resize(output_frames, 0.0);
                let input =
                    InterleavedSlice::new(&self.native_buffer[..input_frames], 1, input_frames)
                        .map_err(|_| ProcessingError::new("audio_processing_buffer_invalid"))?;
                let mut output = InterleavedSlice::new_mut(
                    &mut self.resample_output[..output_frames],
                    1,
                    output_frames,
                )
                .map_err(|_| ProcessingError::new("audio_processing_buffer_invalid"))?;
                let (consumed, produced) = self
                    .resampler
                    .as_mut()
                    .expect("checked above")
                    .process_into_buffer(&input, &mut output, None)
                    .map_err(|_| ProcessingError::new("audio_resampler_failed"))?;
                self.native_buffer.drain(..consumed);
                for sample in &self.resample_output[..produced] {
                    if sample.is_finite() {
                        self.normalized_buffer.push(sample.clamp(-1.0, 1.0));
                    } else {
                        self.normalized_buffer.push(0.0);
                        sanitized = sanitized.saturating_add(1);
                    }
                }
            }
        }

        let mut chunks = Vec::new();
        let mut level_updates = Vec::new();
        while self.normalized_buffer.len() >= NORMALIZED_CHUNK_SAMPLES {
            let samples: Vec<f32> = self
                .normalized_buffer
                .drain(..NORMALIZED_CHUNK_SAMPLES)
                .collect();
            let start_ms = self.timeline_origin_ms.saturating_add(
                self.normalized_chunks_emitted
                    .saturating_mul(NORMALIZED_CHUNK_SAMPLES as u64)
                    .saturating_mul(1_000)
                    / TARGET_SAMPLE_RATE as u64,
            );
            self.normalized_chunks_emitted = self.normalized_chunks_emitted.saturating_add(1);
            level_updates.extend(self.meter.observe(&samples, start_ms));
            chunks.push(ProcessedAudioChunk {
                source: self.source,
                start_ms,
                samples,
            });
        }

        Ok(PipelineOutput {
            chunks,
            non_finite_samples_sanitized: sanitized,
            level_updates,
        })
    }
}

struct PipelineOutput {
    chunks: Vec<ProcessedAudioChunk>,
    non_finite_samples_sanitized: u64,
    level_updates: Vec<LevelDiagnostics>,
}

#[derive(Default)]
struct LevelMeter {
    sum_squares: f64,
    peak: f32,
    clipping: bool,
    samples: usize,
}

impl LevelMeter {
    fn observe(&mut self, input: &[f32], start_ms: u64) -> Vec<LevelDiagnostics> {
        let mut updates = Vec::new();
        for (index, sample) in input.iter().copied().enumerate() {
            let magnitude = sample.abs();
            self.sum_squares += f64::from(sample) * f64::from(sample);
            self.peak = self.peak.max(magnitude);
            self.clipping |= magnitude >= CLIPPING_THRESHOLD;
            self.samples += 1;

            if self.samples == LEVEL_WINDOW_SAMPLES {
                let rms = (self.sum_squares / LEVEL_WINDOW_SAMPLES as f64).sqrt();
                let at_ms = start_ms.saturating_add(
                    u64::try_from(index + 1)
                        .unwrap_or(u64::MAX)
                        .saturating_mul(1_000)
                        / TARGET_SAMPLE_RATE as u64,
                );
                updates.push(LevelDiagnostics {
                    rms_dbfs: amplitude_dbfs(rms),
                    peak_dbfs: amplitude_dbfs(f64::from(self.peak)),
                    clipping: self.clipping,
                    at_ms,
                });
                *self = Self::default();
            }
        }
        updates
    }
}

fn amplitude_dbfs(amplitude: f64) -> f64 {
    if amplitude <= 0.0 {
        LEVEL_FLOOR_DBFS
    } else {
        (20.0 * amplitude.log10()).max(LEVEL_FLOOR_DBFS)
    }
}

fn decode_and_downmix(packet: &AudioPacket) -> Result<(Vec<f32>, u64), ProcessingError> {
    validate_format(&packet.format)?;
    let expected_bytes = usize::try_from(packet.frames)
        .ok()
        .and_then(|frames| frames.checked_mul(packet.format.block_align as usize))
        .ok_or_else(|| ProcessingError::new("audio_processing_packet_size_invalid"))?;
    if packet.bytes.len() != expected_bytes {
        return Err(ProcessingError::new("audio_processing_packet_size_invalid"));
    }

    let channels = packet.format.channels as usize;
    let bytes_per_sample = (packet.format.bits_per_sample / 8) as usize;
    let mut mono = Vec::with_capacity(packet.frames as usize);
    let mut sanitized = 0_u64;

    for frame in packet
        .bytes
        .chunks_exact(packet.format.block_align as usize)
    {
        let mut sum = 0.0_f64;
        for channel in 0..channels {
            let offset = channel * bytes_per_sample;
            let mut sample =
                decode_sample(&frame[offset..offset + bytes_per_sample], &packet.format)?;
            if !sample.is_finite() {
                sample = 0.0;
                sanitized = sanitized.saturating_add(1);
            }
            sum += f64::from(sample);
        }
        mono.push((sum / channels as f64).clamp(-1.0, 1.0) as f32);
    }

    Ok((mono, sanitized))
}

fn validate_format(format: &NativeAudioFormat) -> Result<(), ProcessingError> {
    if !(8_000..=384_000).contains(&format.sample_rate)
        || !(1..=32).contains(&format.channels)
        || format.bits_per_sample == 0
        || !format.bits_per_sample.is_multiple_of(8)
    {
        return Err(ProcessingError::new("audio_processing_format_invalid"));
    }
    let expected_block_align = u32::from(format.channels)
        .checked_mul(u32::from(format.bits_per_sample / 8))
        .ok_or_else(|| ProcessingError::new("audio_processing_format_invalid"))?;
    if format.block_align != expected_block_align {
        return Err(ProcessingError::new("audio_processing_format_invalid"));
    }

    let supported = match format.sample_type {
        NativeSampleType::Float => {
            matches!(format.bits_per_sample, 32 | 64)
                && format.valid_bits_per_sample == format.bits_per_sample
        }
        NativeSampleType::Integer => {
            matches!(format.bits_per_sample, 8 | 16 | 24 | 32)
                && (1..=format.bits_per_sample).contains(&format.valid_bits_per_sample)
                && (format.bits_per_sample != 8 || format.valid_bits_per_sample == 8)
        }
        NativeSampleType::Unknown => false,
    };
    if !supported {
        return Err(ProcessingError::new("audio_processing_format_unsupported"));
    }
    Ok(())
}

fn decode_sample(bytes: &[u8], format: &NativeAudioFormat) -> Result<f32, ProcessingError> {
    match (format.sample_type, format.bits_per_sample) {
        (NativeSampleType::Float, 32) => {
            Ok(f32::from_le_bytes(bytes.try_into().map_err(|_| {
                ProcessingError::new("audio_processing_packet_size_invalid")
            })?))
        }
        (NativeSampleType::Float, 64) => Ok(f64::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| ProcessingError::new("audio_processing_packet_size_invalid"))?,
        ) as f32),
        (NativeSampleType::Integer, 8) => Ok((f32::from(bytes[0]) - 128.0) / 128.0),
        (NativeSampleType::Integer, bits @ (16 | 24 | 32)) => {
            let value = match bits {
                16 => i64::from(i16::from_le_bytes(bytes.try_into().map_err(|_| {
                    ProcessingError::new("audio_processing_packet_size_invalid")
                })?)),
                24 => {
                    let raw = u32::from(bytes[0])
                        | (u32::from(bytes[1]) << 8)
                        | (u32::from(bytes[2]) << 16);
                    i64::from(((raw << 8) as i32) >> 8)
                }
                32 => i64::from(i32::from_le_bytes(bytes.try_into().map_err(|_| {
                    ProcessingError::new("audio_processing_packet_size_invalid")
                })?)),
                _ => unreachable!(),
            };
            let unused_bits = bits - format.valid_bits_per_sample;
            let value = value >> unused_bits;
            let scale = (1_u64 << (format.valid_bits_per_sample - 1)) as f64;
            Ok((value as f64 / scale) as f32)
        }
        _ => Err(ProcessingError::new("audio_processing_format_unsupported")),
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::{
        LEVEL_FLOOR_DBFS, LevelMeter, SourceProcessor, TARGET_SAMPLE_RATE, amplitude_dbfs,
        decode_and_downmix,
    };
    use crate::audio::{
        AudioPacket, AudioSource, EnqueueResult, NativeAudioFormat, NativeSampleType,
        ProcessedAudioChunk, bounded_queue,
    };

    fn format(
        sample_rate: u32,
        channels: u16,
        bits: u16,
        valid_bits: u16,
        sample_type: NativeSampleType,
    ) -> NativeAudioFormat {
        NativeAudioFormat {
            sample_rate,
            channels,
            bits_per_sample: bits,
            valid_bits_per_sample: valid_bits,
            block_align: u32::from(channels) * u32::from(bits / 8),
            channel_mask: 0,
            sample_type,
        }
    }

    fn packet(format: NativeAudioFormat, frames: u32, bytes: Vec<u8>) -> AudioPacket {
        AudioPacket {
            source: AudioSource::Microphone,
            start_ms: 25,
            frames,
            bytes,
            format,
        }
    }

    #[test]
    fn decodes_pcm_container_matrix_and_left_aligned_valid_bits() {
        let cases = [
            (
                format(16_000, 1, 8, 8, NativeSampleType::Integer),
                vec![0, 128, 255],
                vec![-1.0, 0.0, 127.0 / 128.0],
            ),
            (
                format(16_000, 1, 16, 16, NativeSampleType::Integer),
                [-32_768_i16, 0, 16_384]
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect(),
                vec![-1.0, 0.0, 0.5],
            ),
            (
                format(16_000, 1, 24, 24, NativeSampleType::Integer),
                vec![0, 0, 128, 0, 0, 0, 0, 0, 64],
                vec![-1.0, 0.0, 0.5],
            ),
            (
                format(16_000, 1, 32, 32, NativeSampleType::Integer),
                [i32::MIN, 0, 1 << 30]
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .collect(),
                vec![-1.0, 0.0, 0.5],
            ),
            (
                format(16_000, 1, 24, 20, NativeSampleType::Integer),
                vec![0, 0, 128, 0, 0, 0, 0, 0, 64],
                vec![-1.0, 0.0, 0.5],
            ),
        ];

        for (format, bytes, expected) in cases {
            let (decoded, sanitized) =
                decode_and_downmix(&packet(format, expected.len() as u32, bytes)).unwrap();
            assert_eq!(sanitized, 0);
            for (actual, expected) in decoded.iter().zip(expected) {
                assert!((actual - expected).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn decodes_float_matrix_downmixes_and_sanitizes_non_finite_values() {
        let mut float32 = Vec::new();
        for sample in [0.25_f32, 0.75, f32::NAN, 0.5] {
            float32.extend(sample.to_le_bytes());
        }
        let (decoded32, sanitized32) = decode_and_downmix(&packet(
            format(16_000, 2, 32, 32, NativeSampleType::Float),
            2,
            float32,
        ))
        .unwrap();
        assert_eq!(decoded32, vec![0.5, 0.25]);
        assert_eq!(sanitized32, 1);

        let mut float64 = Vec::new();
        for sample in [-0.5_f64, 0.25, 2.0, -2.0] {
            float64.extend(sample.to_le_bytes());
        }
        let (decoded64, sanitized64) = decode_and_downmix(&packet(
            format(16_000, 1, 64, 64, NativeSampleType::Float),
            4,
            float64,
        ))
        .unwrap();
        assert_eq!(decoded64, vec![-0.5, 0.25, 1.0, -1.0]);
        assert_eq!(sanitized64, 0);

        let mut multichannel = Vec::new();
        for sample in [1.0_f32, 0.5, -0.5, -1.0] {
            multichannel.extend(sample.to_le_bytes());
        }
        let (decoded_multichannel, sanitized_multichannel) = decode_and_downmix(&packet(
            format(16_000, 4, 32, 32, NativeSampleType::Float),
            1,
            multichannel,
        ))
        .unwrap();
        assert_eq!(decoded_multichannel, vec![0.0]);
        assert_eq!(sanitized_multichannel, 0);
    }

    #[test]
    fn rejects_unknown_misaligned_or_truncated_packets() {
        let mut invalid = format(16_000, 2, 32, 32, NativeSampleType::Float);
        invalid.block_align = 4;
        assert_eq!(
            decode_and_downmix(&packet(invalid, 1, vec![0; 4]))
                .unwrap_err()
                .code,
            "audio_processing_format_invalid"
        );

        assert_eq!(
            decode_and_downmix(&packet(
                format(16_000, 1, 32, 32, NativeSampleType::Unknown),
                1,
                vec![0; 4],
            ))
            .unwrap_err()
            .code,
            "audio_processing_format_unsupported"
        );

        assert_eq!(
            decode_and_downmix(&packet(
                format(16_000, 1, 16, 16, NativeSampleType::Integer),
                2,
                vec![0; 2],
            ))
            .unwrap_err()
            .code,
            "audio_processing_packet_size_invalid"
        );
    }

    #[test]
    fn resamples_common_rates_to_bounded_16khz_chunks() {
        for sample_rate in [8_000, 16_000, 22_050, 44_100, 48_000, 96_000] {
            let frames = sample_rate / 2;
            let mut bytes = Vec::with_capacity(frames as usize * 4);
            for index in 0..frames {
                let sample = (TAU * 1_000.0 * index as f32 / sample_rate as f32).sin() * 0.5;
                bytes.extend(sample.to_le_bytes());
            }
            let mut processor = SourceProcessor::new(AudioSource::Microphone);
            let outcome = processor
                .process(packet(
                    format(sample_rate, 1, 32, 32, NativeSampleType::Float),
                    frames,
                    bytes,
                ))
                .unwrap();
            assert!(
                outcome
                    .chunks
                    .iter()
                    .all(|chunk| chunk.samples.len() == 160)
            );
            assert!(
                outcome
                    .chunks
                    .iter()
                    .flat_map(|chunk| &chunk.samples)
                    .all(|sample| sample.is_finite() && (-1.0..=1.0).contains(sample))
            );
            let produced = outcome.chunks.len() * 160;
            assert!(produced.abs_diff((TARGET_SAMPLE_RATE / 2) as usize) <= 320);
            assert!(outcome.pending_native_frames < sample_rate.div_ceil(100) as u64);
            assert!(outcome.pending_normalized_samples < 160);
        }
    }

    #[test]
    fn sinc_resampler_attenuates_content_above_the_target_nyquist_limit() {
        let sample_rate = 48_000_u32;
        let frames = sample_rate;
        let mut bytes = Vec::with_capacity(frames as usize * 4);
        for index in 0..frames {
            let sample = (TAU * 12_000.0 * index as f32 / sample_rate as f32).sin() * 0.8;
            bytes.extend(sample.to_le_bytes());
        }
        let mut processor = SourceProcessor::new(AudioSource::Microphone);
        let outcome = processor
            .process(packet(
                format(sample_rate, 1, 32, 32, NativeSampleType::Float),
                frames,
                bytes,
            ))
            .unwrap();
        let samples: Vec<f32> = outcome
            .chunks
            .iter()
            .flat_map(|chunk| chunk.samples.iter().copied())
            .skip(1_600)
            .collect();
        let rms = (samples
            .iter()
            .map(|sample| f64::from(*sample) * f64::from(*sample))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        assert!(rms < 0.05, "out-of-band RMS was {rms}");
    }

    #[test]
    fn level_meter_emits_ten_hertz_rms_peak_and_clipping_windows() {
        let mut meter = LevelMeter::default();
        let half_scale = vec![0.5; 1_600];
        let update = meter.observe(&half_scale, 50).pop().unwrap();
        assert!((update.rms_dbfs + 6.0206).abs() < 0.001);
        assert!((update.peak_dbfs + 6.0206).abs() < 0.001);
        assert!(!update.clipping);
        assert_eq!(update.at_ms, 150);

        let clipped = meter.observe(&vec![1.0; 1_600], 150).pop().unwrap();
        assert_eq!(clipped.rms_dbfs, 0.0);
        assert_eq!(clipped.peak_dbfs, 0.0);
        assert!(clipped.clipping);

        assert_eq!(amplitude_dbfs(0.0), LEVEL_FLOOR_DBFS);
    }

    #[test]
    fn format_changes_rebuild_only_the_source_local_pipeline() {
        let mut processor = SourceProcessor::new(AudioSource::Microphone);
        let first = packet(
            format(16_000, 1, 32, 32, NativeSampleType::Float),
            160,
            vec![0; 640],
        );
        assert!(!processor.process(first).unwrap().format_changed);

        let second = packet(
            format(48_000, 2, 16, 16, NativeSampleType::Integer),
            480,
            vec![0; 1_920],
        );
        assert!(processor.process(second).unwrap().format_changed);

        let wrong_source = AudioPacket {
            source: AudioSource::SystemOutput,
            start_ms: 0,
            frames: 160,
            bytes: vec![0; 640],
            format: format(16_000, 1, 32, 32, NativeSampleType::Float),
        };
        assert_eq!(
            processor.process(wrong_source).unwrap_err().code,
            "audio_processing_source_mismatch"
        );
    }

    #[test]
    fn bounded_normalized_queues_isolate_pressure_by_source() {
        let (microphone_sender, microphone_receiver) = bounded_queue(1);
        let (system_sender, system_receiver) = bounded_queue(1);
        let chunk = |source| ProcessedAudioChunk {
            source,
            start_ms: 0,
            samples: vec![0.0; 160],
        };

        assert_eq!(
            microphone_sender.try_send(chunk(AudioSource::Microphone)),
            EnqueueResult::Enqueued
        );
        assert_eq!(
            microphone_sender.try_send(chunk(AudioSource::Microphone)),
            EnqueueResult::DroppedFull
        );
        assert_eq!(
            system_sender.try_send(chunk(AudioSource::SystemOutput)),
            EnqueueResult::Enqueued
        );
        assert_eq!(microphone_receiver.receiver().len(), 1);
        assert_eq!(system_receiver.receiver().len(), 1);
    }
}
