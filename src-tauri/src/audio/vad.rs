use std::collections::VecDeque;

use earshot::Detector;
use serde::{Deserialize, Serialize};

use super::AudioSource;

pub(crate) const VAD_FRAME_SAMPLES: usize = 256;
pub(crate) const VAD_THRESHOLD: f32 = 0.5;
pub(crate) const PRE_ROLL_SAMPLES: usize = 4_800;
pub(crate) const TRAILING_SILENCE_SAMPLES: usize = 8_000;
pub(crate) const MINIMUM_SPEECH_SAMPLES: usize = 2_560;
pub(crate) const MAX_UTTERANCE_SAMPLES: usize = 480_000;
pub(crate) const MAX_VAD_BUFFERED_SAMPLES: usize =
    MAX_UTTERANCE_SAMPLES + PRE_ROLL_SAMPLES + VAD_FRAME_SAMPLES - 1;
const SAMPLE_RATE: u64 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UtteranceEndReason {
    TrailingSilence,
    ForcedSplit,
    FormatChange,
    TimelineDiscontinuity,
    EndOfStream,
}

#[derive(Debug)]
pub(crate) struct DetectedUtterance {
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) samples: Vec<f32>,
    pub(crate) reason: UtteranceEndReason,
}

#[derive(Debug, Default)]
pub(crate) struct VadProcessOutcome {
    pub(crate) utterances: Vec<DetectedUtterance>,
    pub(crate) frames_analyzed: u64,
    pub(crate) speech_frames: u64,
    pub(crate) silence_frames: u64,
    pub(crate) short_utterances_rejected: u64,
    pub(crate) forced_splits: u64,
    pub(crate) resets: u64,
    pub(crate) pending_samples: u64,
    pub(crate) buffered_samples: u64,
    pub(crate) ordering_watermark_ms: Option<u64>,
}

impl VadProcessOutcome {
    pub(crate) fn merge(&mut self, mut other: Self) {
        self.utterances.append(&mut other.utterances);
        self.frames_analyzed = self.frames_analyzed.saturating_add(other.frames_analyzed);
        self.speech_frames = self.speech_frames.saturating_add(other.speech_frames);
        self.silence_frames = self.silence_frames.saturating_add(other.silence_frames);
        self.short_utterances_rejected = self
            .short_utterances_rejected
            .saturating_add(other.short_utterances_rejected);
        self.forced_splits = self.forced_splits.saturating_add(other.forced_splits);
        self.resets = self.resets.saturating_add(other.resets);
        self.pending_samples = other.pending_samples;
        self.buffered_samples = other.buffered_samples;
        self.ordering_watermark_ms = other.ordering_watermark_ms;
    }
}

pub(crate) trait FramePredictor {
    fn predict(&mut self, frame: &[f32]) -> f32;
    fn reset(&mut self);
}

pub(crate) struct EarshotPredictor(Box<Detector>);

impl Default for EarshotPredictor {
    fn default() -> Self {
        Self(Detector::default_boxed())
    }
}

impl FramePredictor for EarshotPredictor {
    fn predict(&mut self, frame: &[f32]) -> f32 {
        self.0.predict_f32(frame)
    }

    fn reset(&mut self) {
        self.0.reset();
    }
}

struct ActiveUtterance {
    start_ms: u64,
    samples: Vec<f32>,
    speech_samples: usize,
    trailing_silence_samples: usize,
}

pub(crate) struct VadSegmenter<P: FramePredictor = EarshotPredictor> {
    source: AudioSource,
    predictor: P,
    pending: Vec<f32>,
    pending_start_ms: Option<u64>,
    expected_chunk_start_ms: Option<u64>,
    pre_roll: VecDeque<f32>,
    active: Option<ActiveUtterance>,
}

impl VadSegmenter<EarshotPredictor> {
    pub(crate) fn new(source: AudioSource) -> Self {
        Self::with_predictor(source, EarshotPredictor::default())
    }
}

impl<P: FramePredictor> VadSegmenter<P> {
    #[cfg(test)]
    fn with_test_predictor(source: AudioSource, predictor: P) -> Self {
        Self::with_predictor(source, predictor)
    }

    fn with_predictor(source: AudioSource, predictor: P) -> Self {
        Self {
            source,
            predictor,
            pending: Vec::with_capacity(VAD_FRAME_SAMPLES * 2),
            pending_start_ms: None,
            expected_chunk_start_ms: None,
            pre_roll: VecDeque::with_capacity(PRE_ROLL_SAMPLES),
            active: None,
        }
    }

    pub(crate) fn push(
        &mut self,
        source: AudioSource,
        start_ms: u64,
        samples: &[f32],
    ) -> VadProcessOutcome {
        let mut outcome = VadProcessOutcome::default();
        if source != self.source || samples.iter().any(|sample| !(-1.0..=1.0).contains(sample)) {
            outcome.merge(self.reset(UtteranceEndReason::TimelineDiscontinuity));
            return outcome;
        }

        if self
            .expected_chunk_start_ms
            .is_some_and(|expected| expected != start_ms)
        {
            outcome.merge(self.reset(UtteranceEndReason::TimelineDiscontinuity));
        }
        self.expected_chunk_start_ms =
            Some(start_ms.saturating_add(samples.len() as u64 * 1_000 / SAMPLE_RATE));
        if self.pending.is_empty() {
            self.pending_start_ms = Some(start_ms);
        }
        self.pending.extend_from_slice(samples);

        while self.pending.len() >= VAD_FRAME_SAMPLES {
            let frame: Vec<f32> = self.pending.drain(..VAD_FRAME_SAMPLES).collect();
            let frame_start_ms = self.pending_start_ms.unwrap_or(start_ms);
            self.pending_start_ms = if self.pending.is_empty() {
                None
            } else {
                Some(frame_start_ms.saturating_add(16))
            };
            let speech = self.predictor.predict(&frame) >= VAD_THRESHOLD;
            outcome.frames_analyzed = outcome.frames_analyzed.saturating_add(1);
            if speech {
                outcome.speech_frames = outcome.speech_frames.saturating_add(1);
            } else {
                outcome.silence_frames = outcome.silence_frames.saturating_add(1);
            }
            self.observe_frame(&frame, frame_start_ms, speech, &mut outcome);
        }
        self.finish_outcome(&mut outcome);
        outcome
    }

    pub(crate) fn format_changed(&mut self) -> VadProcessOutcome {
        self.reset(UtteranceEndReason::FormatChange)
    }

    pub(crate) fn finish(&mut self) -> VadProcessOutcome {
        self.append_pending_to_active();
        let mut outcome = self.finalize_active(UtteranceEndReason::EndOfStream);
        self.clear_state();
        self.finish_outcome(&mut outcome);
        outcome
    }

    pub(crate) fn snapshot(&self) -> VadProcessOutcome {
        let mut outcome = VadProcessOutcome::default();
        self.finish_outcome(&mut outcome);
        outcome
    }

    fn reset(&mut self, reason: UtteranceEndReason) -> VadProcessOutcome {
        self.append_pending_to_active();
        let mut outcome = self.finalize_active(reason);
        outcome.resets = 1;
        self.clear_state();
        self.finish_outcome(&mut outcome);
        outcome
    }

    fn clear_state(&mut self) {
        self.predictor.reset();
        self.pending.clear();
        self.pending_start_ms = None;
        self.expected_chunk_start_ms = None;
        self.pre_roll.clear();
        self.active = None;
    }

    fn append_pending_to_active(&mut self) {
        if let Some(active) = self.active.as_mut() {
            active.samples.extend_from_slice(&self.pending);
        }
    }

    fn observe_frame(
        &mut self,
        frame: &[f32],
        frame_start_ms: u64,
        speech: bool,
        outcome: &mut VadProcessOutcome,
    ) {
        if self.active.is_none() {
            if !speech {
                self.extend_pre_roll(frame);
                return;
            }
            let mut samples = Vec::with_capacity(MAX_UTTERANCE_SAMPLES);
            samples.extend(self.pre_roll.drain(..));
            samples.extend_from_slice(frame);
            let pre_roll_ms = (samples.len() - frame.len()) as u64 * 1_000 / SAMPLE_RATE;
            self.active = Some(ActiveUtterance {
                start_ms: frame_start_ms.saturating_sub(pre_roll_ms),
                samples,
                speech_samples: frame.len(),
                trailing_silence_samples: 0,
            });
        } else if let Some(active) = self.active.as_mut() {
            active.samples.extend_from_slice(frame);
            if speech {
                active.speech_samples = active.speech_samples.saturating_add(frame.len());
                active.trailing_silence_samples = 0;
            } else {
                active.trailing_silence_samples =
                    active.trailing_silence_samples.saturating_add(frame.len());
            }
        }

        let forced = self
            .active
            .as_ref()
            .is_some_and(|active| active.samples.len() >= MAX_UTTERANCE_SAMPLES);
        if forced {
            if let Some(mut active) = self.active.take() {
                active.samples.truncate(MAX_UTTERANCE_SAMPLES);
                let tail_start = active.samples.len().saturating_sub(PRE_ROLL_SAMPLES);
                self.pre_roll.extend(&active.samples[tail_start..]);
                self.emit(active, UtteranceEndReason::ForcedSplit, outcome);
                outcome.forced_splits = outcome.forced_splits.saturating_add(1);
            }
            return;
        }

        let trailing = self
            .active
            .as_ref()
            .is_some_and(|active| active.trailing_silence_samples >= TRAILING_SILENCE_SAMPLES);
        if trailing && let Some(mut active) = self.active.take() {
            let excess = active
                .trailing_silence_samples
                .saturating_sub(TRAILING_SILENCE_SAMPLES);
            active
                .samples
                .truncate(active.samples.len().saturating_sub(excess));
            self.emit(active, UtteranceEndReason::TrailingSilence, outcome);
        }
    }

    fn extend_pre_roll(&mut self, frame: &[f32]) {
        self.pre_roll.extend(frame.iter().copied());
        while self.pre_roll.len() > PRE_ROLL_SAMPLES {
            self.pre_roll.pop_front();
        }
    }

    fn finalize_active(&mut self, reason: UtteranceEndReason) -> VadProcessOutcome {
        let mut outcome = VadProcessOutcome::default();
        if let Some(active) = self.active.take() {
            self.emit(active, reason, &mut outcome);
        }
        outcome
    }

    fn emit(
        &self,
        active: ActiveUtterance,
        reason: UtteranceEndReason,
        outcome: &mut VadProcessOutcome,
    ) {
        if active.speech_samples < MINIMUM_SPEECH_SAMPLES {
            outcome.short_utterances_rejected = outcome.short_utterances_rejected.saturating_add(1);
            return;
        }
        let duration_ms = active.samples.len() as u64 * 1_000 / SAMPLE_RATE;
        outcome.utterances.push(DetectedUtterance {
            source: self.source,
            start_ms: active.start_ms,
            end_ms: active.start_ms.saturating_add(duration_ms),
            samples: active.samples,
            reason,
        });
    }

    fn finish_outcome(&self, outcome: &mut VadProcessOutcome) {
        outcome.pending_samples = self.pending.len() as u64;
        outcome.buffered_samples = (self.pending.len()
            + self.pre_roll.len()
            + self
                .active
                .as_ref()
                .map_or(0, |active| active.samples.len()))
            as u64;
        outcome.ordering_watermark_ms = self.ordering_watermark_ms();
        debug_assert!(outcome.buffered_samples <= MAX_VAD_BUFFERED_SAMPLES as u64);
    }

    fn ordering_watermark_ms(&self) -> Option<u64> {
        if let Some(active) = self.active.as_ref() {
            return Some(active.start_ms);
        }
        let pending_start_ms = self.pending_start_ms.or(self.expected_chunk_start_ms)?;
        let pre_roll_ms = self.pre_roll.len() as u64 * 1_000 / SAMPLE_RATE;
        Some(pending_start_ms.saturating_sub(pre_roll_ms))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, f32::consts::TAU, time::Instant};

    use silero::{SampleRate, Session, StreamState};

    use super::*;

    struct ScriptedPredictor {
        scores: VecDeque<f32>,
        resets: usize,
    }

    impl ScriptedPredictor {
        fn new(decisions: impl IntoIterator<Item = bool>) -> Self {
            Self {
                scores: decisions
                    .into_iter()
                    .map(|speech| if speech { 0.9 } else { 0.1 })
                    .collect(),
                resets: 0,
            }
        }
    }

    impl FramePredictor for ScriptedPredictor {
        fn predict(&mut self, _frame: &[f32]) -> f32 {
            self.scores.pop_front().expect("scripted score")
        }

        fn reset(&mut self) {
            self.resets += 1;
        }
    }

    fn run_script(decisions: Vec<bool>) -> (VadSegmenter<ScriptedPredictor>, VadProcessOutcome) {
        assert_eq!(decisions.len() % 5, 0);
        let mut segmenter = VadSegmenter::with_test_predictor(
            AudioSource::Microphone,
            ScriptedPredictor::new(decisions.clone()),
        );
        let mut outcome = VadProcessOutcome::default();
        for chunk_index in 0..decisions.len() / 5 * 8 {
            outcome.merge(segmenter.push(
                AudioSource::Microphone,
                chunk_index as u64 * 10,
                &[0.0; 160],
            ));
        }
        (segmenter, outcome)
    }

    #[test]
    fn trailing_silence_preserves_exact_pre_roll_and_tail() {
        let decisions = [vec![false; 20], vec![true; 10], vec![false; 35]].concat();
        let (_, outcome) = run_script(decisions);
        assert_eq!(outcome.utterances.len(), 1);
        let utterance = &outcome.utterances[0];
        assert_eq!(utterance.start_ms, 20);
        assert_eq!(utterance.end_ms, 980);
        assert_eq!(utterance.samples.len(), 15_360);
        assert_eq!(utterance.reason, UtteranceEndReason::TrailingSilence);
    }

    #[test]
    fn ordering_watermark_tracks_pre_roll_active_speech_and_finalization() {
        let (_, silence) = run_script(vec![false; 20]);
        assert_eq!(silence.ordering_watermark_ms, Some(20));

        let (_, active) = run_script([vec![false; 20], vec![true; 10]].concat());
        assert_eq!(active.ordering_watermark_ms, Some(20));

        let (_, finalized) =
            run_script([vec![false; 20], vec![true; 10], vec![false; 35]].concat());
        assert_eq!(finalized.utterances.len(), 1);
        assert_eq!(finalized.ordering_watermark_ms, Some(992));
    }

    #[test]
    fn rejects_short_speech_and_keeps_memory_bounded() {
        let decisions = [vec![false; 20], vec![true; 5], vec![false; 35]].concat();
        let (_, outcome) = run_script(decisions);
        assert!(outcome.utterances.is_empty());
        assert_eq!(outcome.short_utterances_rejected, 1);
        assert!(outcome.buffered_samples <= MAX_VAD_BUFFERED_SAMPLES as u64);
    }

    #[test]
    fn hard_split_is_exact_and_carries_bounded_overlap() {
        let decisions = [vec![false; 20], vec![true; 1_880]].concat();
        let (mut segmenter, mut outcome) = run_script(decisions);
        outcome.merge(segmenter.finish());
        assert_eq!(outcome.forced_splits, 1);
        assert_eq!(outcome.utterances[0].samples.len(), MAX_UTTERANCE_SAMPLES);
        assert_eq!(
            outcome.utterances[0].reason,
            UtteranceEndReason::ForcedSplit
        );
        assert_eq!(
            outcome.utterances[1].reason,
            UtteranceEndReason::EndOfStream
        );
        assert!(outcome.buffered_samples <= MAX_VAD_BUFFERED_SAMPLES as u64);
    }

    #[test]
    fn discontinuity_and_format_change_finalize_and_reset_source_local_state() {
        let decisions = [vec![true; 10], vec![true; 10]].concat();
        let (mut segmenter, mut outcome) = run_script(decisions);
        outcome.merge(segmenter.push(AudioSource::Microphone, 999, &[0.0; 160]));
        assert_eq!(outcome.resets, 1);
        assert_eq!(
            outcome.utterances[0].reason,
            UtteranceEndReason::TimelineDiscontinuity
        );
        let format = segmenter.format_changed();
        assert_eq!(format.resets, 1);
        assert_eq!(segmenter.predictor.resets, 2);
    }

    #[test]
    fn independent_segmenters_do_not_share_detector_state() {
        let (mut microphone, microphone_outcome) = run_script(vec![true; 10]);
        let (mut system, system_outcome) = run_script(vec![false; 10]);
        assert_eq!(microphone_outcome.speech_frames, 10);
        assert_eq!(system_outcome.speech_frames, 0);
        assert_eq!(microphone.finish().utterances.len(), 1);
        assert!(system.finish().utterances.is_empty());
    }

    #[test]
    fn finish_keeps_the_bounded_subframe_tail() {
        let mut segmenter = VadSegmenter::with_test_predictor(
            AudioSource::Microphone,
            ScriptedPredictor::new(vec![true; 10]),
        );
        for chunk_index in 0..17 {
            let outcome = segmenter.push(AudioSource::Microphone, chunk_index * 10, &[0.0; 160]);
            assert!(outcome.utterances.is_empty());
        }
        let outcome = segmenter.finish();
        assert_eq!(outcome.utterances.len(), 1);
        assert_eq!(outcome.utterances[0].samples.len(), 2_720);
        assert_eq!(outcome.utterances[0].end_ms, 170);
        assert_eq!(outcome.pending_samples, 0);
        assert_eq!(outcome.buffered_samples, 0);
    }

    struct CorpusCase {
        name: &'static str,
        speech: bool,
        samples: Vec<f32>,
    }

    fn deterministic_corpus() -> Vec<CorpusCase> {
        let tone = |frequency: f32, noise: f32| {
            (0..64_000)
                .map(|index| {
                    let time = index as f32 / 16_000.0;
                    let envelope = (TAU * 3.0 * time).sin().abs().max(0.08);
                    let harmonic = (TAU * frequency * time).sin()
                        + 0.45 * (TAU * frequency * 2.0 * time).sin()
                        + 0.2 * (TAU * frequency * 3.0 * time).sin();
                    let pseudo_noise = (((index as u32)
                        .wrapping_mul(1_664_525)
                        .wrapping_add(1_013_904_223)
                        >> 8) as f32
                        / 16_777_215.0
                        - 0.5)
                        * noise;
                    (harmonic * envelope * 0.35 + pseudo_noise).clamp(-1.0, 1.0)
                })
                .collect()
        };
        let noise = (0..64_000)
            .map(|index| {
                (((index as u32)
                    .wrapping_mul(1_103_515_245)
                    .wrapping_add(12_345)
                    >> 8) as f32
                    / 16_777_215.0
                    - 0.5)
                    * 0.08
            })
            .collect();
        let two_tone = (0..64_000)
            .map(|index| {
                let time = index as f32 / 16_000.0;
                0.25 * ((TAU * 2_500.0 * time).sin() + (TAU * 3_500.0 * time).sin())
            })
            .collect();
        vec![
            CorpusCase {
                name: "speech_proxy_clean",
                speech: true,
                samples: tone(140.0, 0.0),
            },
            CorpusCase {
                name: "speech_proxy_noisy",
                speech: true,
                samples: tone(210.0, 0.04),
            },
            CorpusCase {
                name: "silence",
                speech: false,
                samples: vec![0.0; 64_000],
            },
            CorpusCase {
                name: "deterministic_noise",
                speech: false,
                samples: noise,
            },
            CorpusCase {
                name: "two_tone",
                speech: false,
                samples: two_tone,
            },
        ]
    }

    #[derive(Default)]
    struct Counts {
        tp: u64,
        fp: u64,
        tn: u64,
        fn_: u64,
    }

    impl Counts {
        fn observe(&mut self, expected: bool, actual: bool) {
            match (expected, actual) {
                (true, true) => self.tp += 1,
                (false, true) => self.fp += 1,
                (false, false) => self.tn += 1,
                (true, false) => self.fn_ += 1,
            }
        }
        fn miss_rate(&self) -> f64 {
            self.fn_ as f64 / (self.tp + self.fn_) as f64
        }
        fn false_positive_rate(&self) -> f64 {
            self.fp as f64 / (self.fp + self.tn) as f64
        }
        fn f1(&self) -> f64 {
            2.0 * self.tp as f64 / (2 * self.tp + self.fp + self.fn_) as f64
        }
    }

    #[test]
    #[ignore = "explicit P3-003 dependency/model bake-off"]
    fn earshot_meets_frozen_quality_gates_against_silero_v6() {
        let corpus = deterministic_corpus();
        let mut earshot = Detector::default_boxed();
        let mut silero = Session::bundled().expect("bundled Silero v6 model");
        let mut earshot_counts = Counts::default();
        let mut silero_counts = Counts::default();
        let earshot_started = Instant::now();
        for case in &corpus {
            earshot.reset();
            for window in case.samples.chunks_exact(512) {
                let score = earshot
                    .predict_f32(&window[..256])
                    .max(earshot.predict_f32(&window[256..]));
                earshot_counts.observe(case.speech, score >= VAD_THRESHOLD);
            }
        }
        let earshot_elapsed = earshot_started.elapsed();
        let silero_started = Instant::now();
        for case in &corpus {
            let mut state = StreamState::new(SampleRate::Rate16k);
            for window in case.samples.chunks_exact(512) {
                let score = silero
                    .infer_chunk(&mut state, window)
                    .expect("Silero inference");
                silero_counts.observe(case.speech, score >= VAD_THRESHOLD);
            }
            println!(
                "corpus_case={} windows={}",
                case.name,
                case.samples.len() / 512
            );
        }
        let silero_elapsed = silero_started.elapsed();
        println!(
            "earshot miss={:.4} fp={:.4} f1={:.4} elapsed_ms={} | silero miss={:.4} fp={:.4} f1={:.4} elapsed_ms={}",
            earshot_counts.miss_rate(),
            earshot_counts.false_positive_rate(),
            earshot_counts.f1(),
            earshot_elapsed.as_millis(),
            silero_counts.miss_rate(),
            silero_counts.false_positive_rate(),
            silero_counts.f1(),
            silero_elapsed.as_millis()
        );
        assert!(earshot_counts.miss_rate() <= 0.10);
        assert!(earshot_counts.false_positive_rate() <= 0.05);
        assert!(earshot_counts.f1() + 0.02 >= silero_counts.f1());
    }
}
