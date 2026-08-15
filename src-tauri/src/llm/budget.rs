use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use crate::{
    domain::{OpenRouterModel, RequestId, Session, SessionId},
    prompts::{PromptEnvelope, PromptPurpose},
};

use super::openrouter::OpenRouterService;

const USD_SCALE_DIGITS: usize = 12;
const USD_SCALE: u128 = 1_000_000_000_000;
const MAX_TRACKED_SESSIONS: usize = 128;
const MAX_PENDING_RESERVATIONS: usize = 256;
const MAX_PENDING_RESERVATIONS_PER_SESSION: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageReservation {
    pub(crate) request_id: RequestId,
    pub(crate) session_id: SessionId,
    pub(crate) purpose: PromptPurpose,
    pub(crate) model_id: String,
    pub(crate) input_token_ceiling: u32,
    pub(crate) output_token_ceiling: u32,
    pub(crate) reserved_cost_usd: String,
    pub(crate) available_budget_usd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FinalUsage {
    pub(crate) request_id: RequestId,
    pub(crate) session_id: SessionId,
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageReconciliation {
    pub(crate) request_id: RequestId,
    pub(crate) session_id: SessionId,
    pub(crate) purpose: PromptPurpose,
    pub(crate) model_id: String,
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) reserved_cost_usd: String,
    pub(crate) actual_cost_usd: String,
    pub(crate) released_cost_usd: String,
    pub(crate) session_actual_cost_usd: String,
    pub(crate) available_budget_usd: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageRelease {
    pub(crate) request_id: RequestId,
    pub(crate) session_id: SessionId,
    pub(crate) released_cost_usd: String,
    pub(crate) session_actual_cost_usd: String,
    pub(crate) available_budget_usd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsageBudgetError {
    pub(crate) code: &'static str,
}

impl UsageBudgetError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for UsageBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for UsageBudgetError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct ScaledUsd(u128);

impl ScaledUsd {
    fn checked_add(self, other: Self) -> Result<Self, UsageBudgetError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(|| UsageBudgetError::new("usage_cost_overflow"))
    }

    fn checked_sub(self, other: Self) -> Result<Self, UsageBudgetError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| UsageBudgetError::new("usage_ledger_invalid"))
    }

    fn checked_mul_tokens(self, tokens: u32) -> Result<Self, UsageBudgetError> {
        self.0
            .checked_mul(u128::from(tokens))
            .map(Self)
            .ok_or_else(|| UsageBudgetError::new("usage_cost_overflow"))
    }

    fn canonical(self) -> String {
        let whole = self.0 / USD_SCALE;
        let fraction = self.0 % USD_SCALE;
        format!("{whole}.{fraction:0USD_SCALE_DIGITS$}")
    }
}

#[derive(Clone, Default)]
pub(super) struct UsageBudgetService {
    state: Arc<Mutex<BudgetState>>,
}

#[derive(Default)]
struct BudgetState {
    sessions: HashMap<SessionId, SessionLedger>,
    pending: HashMap<RequestId, PendingReservation>,
}

#[derive(Debug, Clone, Copy)]
struct SessionLedger {
    limit: ScaledUsd,
    baseline_actual: ScaledUsd,
    reconciled_actual: ScaledUsd,
    reserved: ScaledUsd,
    pending_count: usize,
}

#[derive(Debug, Clone)]
struct PendingReservation {
    session_id: SessionId,
    purpose: PromptPurpose,
    model_id: String,
    input_token_ceiling: u32,
    output_token_ceiling: u32,
    input_rate: ScaledUsd,
    output_rate: ScaledUsd,
    reserved_cost: ScaledUsd,
}

impl OpenRouterService {
    pub(crate) fn reserve_prompt_usage(
        &self,
        request_id: RequestId,
        session: &Session,
        envelope: &PromptEnvelope,
    ) -> Result<UsageReservation, UsageBudgetError> {
        let model = self
            .cached_model_for_pricing(&envelope.model_id)
            .map_err(UsageBudgetError::new)?;
        self.usage_budget
            .reserve(request_id, session, envelope, &model)
    }

    pub(crate) fn reconcile_prompt_usage(
        &self,
        usage: FinalUsage,
    ) -> Result<UsageReconciliation, UsageBudgetError> {
        self.usage_budget.reconcile(usage)
    }

    pub(crate) fn release_prompt_usage(
        &self,
        request_id: RequestId,
        session_id: SessionId,
    ) -> Result<UsageRelease, UsageBudgetError> {
        self.usage_budget.release(request_id, session_id)
    }
}

impl UsageBudgetService {
    fn reserve(
        &self,
        request_id: RequestId,
        session: &Session,
        envelope: &PromptEnvelope,
        model: &OpenRouterModel,
    ) -> Result<UsageReservation, UsageBudgetError> {
        validate_reservation_input(session, envelope, model)?;
        let limit = parse_session_amount(&session.spending_limit_usd)?;
        let baseline_actual = parse_session_amount(&session.usage.actual_cost_usd)?;
        let input_rate = parse_provider_price(&model.prompt_price_per_token)?;
        let output_rate = parse_provider_price(&model.completion_price_per_token)?;
        let input_cost = input_rate.checked_mul_tokens(envelope.input_token_limit)?;
        let output_cost = output_rate.checked_mul_tokens(envelope.maximum_output_tokens)?;
        let reserved_cost = input_cost.checked_add(output_cost)?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| UsageBudgetError::new("usage_ledger_unavailable"))?;
        if state.pending.contains_key(&request_id) {
            return Err(UsageBudgetError::new("usage_request_duplicate"));
        }
        if state.pending.len() >= MAX_PENDING_RESERVATIONS {
            return Err(UsageBudgetError::new("usage_reservation_capacity_reached"));
        }

        let existing = state.sessions.get(&session.id).copied();
        if let Some(ledger) = existing {
            if ledger.limit != limit || ledger.baseline_actual != baseline_actual {
                return Err(UsageBudgetError::new("usage_session_snapshot_changed"));
            }
            if ledger.pending_count >= MAX_PENDING_RESERVATIONS_PER_SESSION {
                return Err(UsageBudgetError::new("usage_session_capacity_reached"));
            }
        } else if state.sessions.len() >= MAX_TRACKED_SESSIONS {
            return Err(UsageBudgetError::new("usage_session_capacity_reached"));
        }

        let ledger = existing.unwrap_or(SessionLedger {
            limit,
            baseline_actual,
            reconciled_actual: ScaledUsd::default(),
            reserved: ScaledUsd::default(),
            pending_count: 0,
        });
        let actual = ledger
            .baseline_actual
            .checked_add(ledger.reconciled_actual)?;
        let projected_reserved = ledger.reserved.checked_add(reserved_cost)?;
        let projected_total = actual.checked_add(projected_reserved)?;
        if reserved_cost != ScaledUsd::default() && projected_total > ledger.limit {
            return Err(UsageBudgetError::new("usage_session_limit_exceeded"));
        }
        let available = available_budget(ledger.limit, projected_total);

        let ledger = state.sessions.entry(session.id).or_insert(ledger);
        ledger.reserved = projected_reserved;
        ledger.pending_count += 1;
        state.pending.insert(
            request_id,
            PendingReservation {
                session_id: session.id,
                purpose: envelope.specification.purpose,
                model_id: envelope.model_id.clone(),
                input_token_ceiling: envelope.input_token_limit,
                output_token_ceiling: envelope.maximum_output_tokens,
                input_rate,
                output_rate,
                reserved_cost,
            },
        );

        Ok(UsageReservation {
            request_id,
            session_id: session.id,
            purpose: envelope.specification.purpose,
            model_id: envelope.model_id.clone(),
            input_token_ceiling: envelope.input_token_limit,
            output_token_ceiling: envelope.maximum_output_tokens,
            reserved_cost_usd: reserved_cost.canonical(),
            available_budget_usd: available.canonical(),
        })
    }

    fn reconcile(&self, usage: FinalUsage) -> Result<UsageReconciliation, UsageBudgetError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| UsageBudgetError::new("usage_ledger_unavailable"))?;
        let pending = state
            .pending
            .get(&usage.request_id)
            .cloned()
            .ok_or_else(|| UsageBudgetError::new("usage_reservation_not_found"))?;
        if pending.session_id != usage.session_id {
            return Err(UsageBudgetError::new("usage_reservation_scope_invalid"));
        }
        if usage.input_tokens > pending.input_token_ceiling
            || usage.output_tokens > pending.output_token_ceiling
        {
            return Err(UsageBudgetError::new(
                "usage_final_tokens_exceed_reservation",
            ));
        }

        let actual_cost = pending
            .input_rate
            .checked_mul_tokens(usage.input_tokens)?
            .checked_add(
                pending
                    .output_rate
                    .checked_mul_tokens(usage.output_tokens)?,
            )?;
        if actual_cost > pending.reserved_cost {
            return Err(UsageBudgetError::new(
                "usage_final_cost_exceeds_reservation",
            ));
        }
        let released_cost = pending.reserved_cost.checked_sub(actual_cost)?;

        let (session_actual, available) = {
            let ledger = state
                .sessions
                .get_mut(&usage.session_id)
                .ok_or_else(|| UsageBudgetError::new("usage_ledger_invalid"))?;
            ledger.reserved = ledger.reserved.checked_sub(pending.reserved_cost)?;
            ledger.reconciled_actual = ledger.reconciled_actual.checked_add(actual_cost)?;
            ledger.pending_count = ledger
                .pending_count
                .checked_sub(1)
                .ok_or_else(|| UsageBudgetError::new("usage_ledger_invalid"))?;
            let session_actual = ledger
                .baseline_actual
                .checked_add(ledger.reconciled_actual)?;
            let committed_and_pending = session_actual.checked_add(ledger.reserved)?;
            (
                session_actual,
                available_budget(ledger.limit, committed_and_pending),
            )
        };
        state.pending.remove(&usage.request_id);

        Ok(UsageReconciliation {
            request_id: usage.request_id,
            session_id: usage.session_id,
            purpose: pending.purpose,
            model_id: pending.model_id,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            reserved_cost_usd: pending.reserved_cost.canonical(),
            actual_cost_usd: actual_cost.canonical(),
            released_cost_usd: released_cost.canonical(),
            session_actual_cost_usd: session_actual.canonical(),
            available_budget_usd: available.canonical(),
        })
    }

    fn release(
        &self,
        request_id: RequestId,
        session_id: SessionId,
    ) -> Result<UsageRelease, UsageBudgetError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| UsageBudgetError::new("usage_ledger_unavailable"))?;
        let pending = state
            .pending
            .get(&request_id)
            .cloned()
            .ok_or_else(|| UsageBudgetError::new("usage_reservation_not_found"))?;
        if pending.session_id != session_id {
            return Err(UsageBudgetError::new("usage_reservation_scope_invalid"));
        }

        let (session_actual, available) = {
            let ledger = state
                .sessions
                .get_mut(&session_id)
                .ok_or_else(|| UsageBudgetError::new("usage_ledger_invalid"))?;
            ledger.reserved = ledger.reserved.checked_sub(pending.reserved_cost)?;
            ledger.pending_count = ledger
                .pending_count
                .checked_sub(1)
                .ok_or_else(|| UsageBudgetError::new("usage_ledger_invalid"))?;
            let session_actual = ledger
                .baseline_actual
                .checked_add(ledger.reconciled_actual)?;
            let committed_and_pending = session_actual.checked_add(ledger.reserved)?;
            (
                session_actual,
                available_budget(ledger.limit, committed_and_pending),
            )
        };
        state.pending.remove(&request_id);

        Ok(UsageRelease {
            request_id,
            session_id,
            released_cost_usd: pending.reserved_cost.canonical(),
            session_actual_cost_usd: session_actual.canonical(),
            available_budget_usd: available.canonical(),
        })
    }
}

fn validate_reservation_input(
    session: &Session,
    envelope: &PromptEnvelope,
    model: &OpenRouterModel,
) -> Result<(), UsageBudgetError> {
    session
        .validate()
        .map_err(|_| UsageBudgetError::new("usage_session_invalid"))?;
    model
        .validate()
        .map_err(|_| UsageBudgetError::new("usage_model_record_invalid"))?;
    let selected_model = match envelope.specification.purpose {
        PromptPurpose::Insight => session.llm_models.insights.as_deref(),
        PromptPurpose::Summary => session.llm_models.summaries.as_deref(),
        PromptPurpose::ManualQuestion => session.llm_models.manual_questions.as_deref(),
    };
    if model.id != envelope.model_id || selected_model != Some(envelope.model_id.as_str()) {
        return Err(UsageBudgetError::new("usage_model_mismatch"));
    }
    if envelope.request_token_limit > session.max_tokens_per_request
        || envelope.request_token_limit > envelope.specification.approximate_request_token_limit
        || envelope.maximum_output_tokens > envelope.specification.maximum_output_tokens
        || envelope.estimated_input_tokens > envelope.input_token_limit
        || envelope
            .input_token_limit
            .checked_add(envelope.maximum_output_tokens)
            != Some(envelope.request_token_limit)
    {
        return Err(UsageBudgetError::new("usage_prompt_envelope_invalid"));
    }
    if u64::from(envelope.request_token_limit) > model.context_length {
        return Err(UsageBudgetError::new("usage_model_context_too_small"));
    }
    Ok(())
}

fn parse_session_amount(value: &str) -> Result<ScaledUsd, UsageBudgetError> {
    let (whole, fraction) = value
        .split_once('.')
        .ok_or_else(|| UsageBudgetError::new("usage_session_amount_invalid"))?;
    if value.len() > 18
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() != 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(UsageBudgetError::new("usage_session_amount_invalid"));
    }
    let whole = whole
        .parse::<u128>()
        .map_err(|_| UsageBudgetError::new("usage_session_amount_invalid"))?;
    let fraction = fraction
        .parse::<u128>()
        .map_err(|_| UsageBudgetError::new("usage_session_amount_invalid"))?;
    whole
        .checked_mul(USD_SCALE)
        .and_then(|amount| amount.checked_add(fraction * (USD_SCALE / 100)))
        .map(ScaledUsd)
        .ok_or_else(|| UsageBudgetError::new("usage_cost_overflow"))
}

fn parse_provider_price(value: &str) -> Result<ScaledUsd, UsageBudgetError> {
    if value.is_empty() || value.len() > 64 || value.trim() != value || value.starts_with('-') {
        return Err(UsageBudgetError::new("usage_model_price_invalid"));
    }
    let unsigned = value.strip_prefix('+').unwrap_or(value);
    let exponent_index = unsigned.find(['e', 'E']);
    let (mantissa, exponent) = if let Some(index) = exponent_index {
        let (mantissa, exponent) = unsigned.split_at(index);
        let exponent = &exponent[1..];
        if exponent.is_empty() || exponent.contains(['e', 'E']) {
            return Err(UsageBudgetError::new("usage_model_price_invalid"));
        }
        let exponent = exponent
            .parse::<i32>()
            .map_err(|_| UsageBudgetError::new("usage_model_price_invalid"))?;
        (mantissa, exponent)
    } else {
        (unsigned, 0)
    };
    let mut parts = mantissa.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || (whole.is_empty() && fraction.is_empty())
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(UsageBudgetError::new("usage_model_price_invalid"));
    }

    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok(ScaledUsd::default());
    }
    let shift = i64::try_from(USD_SCALE_DIGITS)
        .expect("scale digits fit i64")
        .checked_add(i64::from(exponent))
        .and_then(|value| value.checked_sub(i64::try_from(fraction.len()).ok()?))
        .ok_or_else(|| UsageBudgetError::new("usage_cost_overflow"))?;

    let scaled = if shift >= 0 {
        let mut scaled = significant
            .parse::<u128>()
            .map_err(|_| UsageBudgetError::new("usage_cost_overflow"))?;
        for _ in
            0..usize::try_from(shift).map_err(|_| UsageBudgetError::new("usage_cost_overflow"))?
        {
            scaled = scaled
                .checked_mul(10)
                .ok_or_else(|| UsageBudgetError::new("usage_cost_overflow"))?;
        }
        scaled
    } else {
        let discarded =
            usize::try_from(-shift).map_err(|_| UsageBudgetError::new("usage_cost_overflow"))?;
        if discarded >= significant.len() {
            1
        } else {
            let kept = significant.len() - discarded;
            let (whole_units, remainder) = significant.split_at(kept);
            let units = whole_units
                .parse::<u128>()
                .map_err(|_| UsageBudgetError::new("usage_cost_overflow"))?;
            if remainder.bytes().any(|byte| byte != b'0') {
                units
                    .checked_add(1)
                    .ok_or_else(|| UsageBudgetError::new("usage_cost_overflow"))?
            } else {
                units
            }
        }
    };
    Ok(ScaledUsd(scaled))
}

fn available_budget(limit: ScaledUsd, committed_and_pending: ScaledUsd) -> ScaledUsd {
    ScaledUsd(limit.0.saturating_sub(committed_and_pending.0))
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use serde_json::json;

    use super::*;
    use crate::{
        domain::OpenRouterDataCollection,
        prompts::{PromptFallbackStrategy, prompt_specifications},
        security::CredentialService,
    };

    fn sample_session(id: &str, budget: &str, actual: &str) -> Session {
        let suffix = id.split('-').next().unwrap();
        serde_json::from_value(json!({
            "schemaVersion": 1,
            "id": id,
            "projectId": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "folderName": format!("2026-08-15-budget-test--{suffix}"),
            "title": "Budget test",
            "objective": "Verify cost accounting.",
            "sessionContext": "",
            "preset": {
                "id": "00000000-0000-4000-8000-000000000001",
                "version": 1,
                "name": "Technical interview",
                "assistantRole": "Track decisions.",
                "analysisObjectives": ["Track decisions"],
                "insightTypes": ["decision"],
                "responseTone": "Concise",
                "finalSummarySections": ["Decisions"],
                "highlightInstructions": [],
                "prohibitedBehaviors": []
            },
            "language": "en-GB",
            "microphone": {
                "endpointId": "microphone-endpoint",
                "friendlyName": "Microphone",
                "selection": {"kind": "fixed", "endpointId": "microphone-endpoint"},
                "nativeSampleRate": 48000,
                "nativeChannels": 1
            },
            "systemOutput": {
                "endpointId": "render-endpoint",
                "friendlyName": "Speakers",
                "selection": {"kind": "default", "role": "console"},
                "nativeSampleRate": 48000,
                "nativeChannels": 2
            },
            "transcriptionEngine": "whisper",
            "transcriptionModelId": "whisper-tiny-multilingual",
            "llmModels": {"insights": "example/text-model"},
            "spendingLimitUsd": budget,
            "maxTokensPerRequest": 4096,
            "retainAudio": false,
            "state": "idle",
            "channelHealth": {
                "microphone": {"status": "stopped", "updatedAt": "2026-08-15T10:00:00Z"},
                "systemOutput": {"status": "stopped", "updatedAt": "2026-08-15T10:00:00Z"}
            },
            "summaryStatus": "not_requested",
            "usage": {
                "inputTokens": 0,
                "outputTokens": 0,
                "estimatedCostUsd": actual,
                "actualCostUsd": actual
            },
            "createdAt": "2026-08-15T10:00:00Z",
            "updatedAt": "2026-08-15T10:00:00Z",
            "revision": 1
        }))
        .unwrap()
    }

    fn model(prompt_price: &str, completion_price: &str, context_length: u64) -> OpenRouterModel {
        OpenRouterModel {
            id: "example/text-model".to_owned(),
            name: "Example".to_owned(),
            provider: "example".to_owned(),
            context_length,
            prompt_price_per_token: prompt_price.to_owned(),
            completion_price_per_token: completion_price.to_owned(),
            supports_structured_outputs: true,
            supports_streaming: true,
            zero_data_retention_available: true,
            data_collection: OpenRouterDataCollection::Deny,
        }
    }

    fn envelope(input: u32, output: u32) -> PromptEnvelope {
        let specification = prompt_specifications()
            .iter()
            .find(|specification| specification.purpose == PromptPurpose::Insight)
            .unwrap();
        PromptEnvelope {
            specification,
            model_id: "example/text-model".to_owned(),
            request_token_limit: input + output,
            input_token_limit: input,
            maximum_output_tokens: output,
            estimated_input_tokens: input / 2,
            selected_segment_ids: Vec::new(),
            omitted_relevant_segments: 0,
            omitted_recent_segments: 0,
            system_instructions: "fixed",
            task_instructions: "fixed",
            untrusted_context: "{}".to_owned(),
            output_schema: "{}".to_owned(),
            fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        }
    }

    #[test]
    fn decimal_prices_are_exact_and_rounded_up_to_picodollars() {
        assert_eq!(parse_provider_price("0").unwrap(), ScaledUsd(0));
        assert_eq!(
            parse_provider_price("0.000001").unwrap(),
            ScaledUsd(1_000_000)
        );
        assert_eq!(parse_provider_price("1e-13").unwrap(), ScaledUsd(1));
        assert_eq!(
            parse_provider_price("1.2345678901234").unwrap(),
            ScaledUsd(1_234_567_890_124)
        );
        assert_eq!(
            parse_provider_price("1e2").unwrap(),
            ScaledUsd(100 * USD_SCALE)
        );
        for invalid in ["-0", "NaN", "1.2.3", "1e", "1e999"] {
            assert!(parse_provider_price(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn reservation_reconciliation_and_release_are_exactly_once() {
        let service = UsageBudgetService::default();
        let session = sample_session("cccccccc-cccc-4ccc-8ccc-cccccccccccc", "0.01", "0.00");
        let envelope = envelope(1_000, 1_000);
        let model = model("0.000001", "0.000002", 4_096);
        let first_id = RequestId::new();

        let reservation = service
            .reserve(first_id, &session, &envelope, &model)
            .unwrap();
        assert_eq!(reservation.reserved_cost_usd, "0.003000000000");
        assert_eq!(reservation.available_budget_usd, "0.007000000000");
        assert_eq!(
            service
                .reserve(first_id, &session, &envelope, &model)
                .unwrap_err()
                .code,
            "usage_request_duplicate"
        );

        let other_session = sample_session("dddddddd-dddd-4ddd-8ddd-dddddddddddd", "0.01", "0.00");
        assert_eq!(
            service
                .reconcile(FinalUsage {
                    request_id: first_id,
                    session_id: other_session.id,
                    input_tokens: 100,
                    output_tokens: 50,
                })
                .unwrap_err()
                .code,
            "usage_reservation_scope_invalid"
        );
        let reconciliation = service
            .reconcile(FinalUsage {
                request_id: first_id,
                session_id: session.id,
                input_tokens: 100,
                output_tokens: 50,
            })
            .unwrap();
        assert_eq!(reconciliation.actual_cost_usd, "0.000200000000");
        assert_eq!(reconciliation.released_cost_usd, "0.002800000000");
        assert_eq!(reconciliation.available_budget_usd, "0.009800000000");
        assert_eq!(
            service
                .reconcile(FinalUsage {
                    request_id: first_id,
                    session_id: session.id,
                    input_tokens: 100,
                    output_tokens: 50,
                })
                .unwrap_err()
                .code,
            "usage_reservation_not_found"
        );

        let second_id = RequestId::new();
        service
            .reserve(second_id, &session, &envelope, &model)
            .unwrap();
        let release = service.release(second_id, session.id).unwrap();
        assert_eq!(release.released_cost_usd, "0.003000000000");
        assert_eq!(release.session_actual_cost_usd, "0.000200000000");
        assert_eq!(
            service.release(second_id, session.id).unwrap_err().code,
            "usage_reservation_not_found"
        );
    }

    #[test]
    fn limits_free_models_and_invalid_final_usage_fail_closed() {
        let paid = model("0.000001", "0", 4_096);
        let free = model("0", "0", 4_096);
        let zero_budget = sample_session("cccccccc-cccc-4ccc-8ccc-cccccccccccc", "0.00", "0.00");
        let envelope = envelope(1_000, 100);
        let service = UsageBudgetService::default();
        assert_eq!(
            service
                .reserve(RequestId::new(), &zero_budget, &envelope, &paid)
                .unwrap_err()
                .code,
            "usage_session_limit_exceeded"
        );
        let free_id = RequestId::new();
        assert_eq!(
            service
                .reserve(free_id, &zero_budget, &envelope, &free)
                .unwrap()
                .reserved_cost_usd,
            "0.000000000000"
        );
        assert_eq!(
            service
                .reconcile(FinalUsage {
                    request_id: free_id,
                    session_id: zero_budget.id,
                    input_tokens: 1_001,
                    output_tokens: 0,
                })
                .unwrap_err()
                .code,
            "usage_final_tokens_exceed_reservation"
        );
        assert!(
            service
                .reconcile(FinalUsage {
                    request_id: free_id,
                    session_id: zero_budget.id,
                    input_tokens: 1_000,
                    output_tokens: 100,
                })
                .is_ok()
        );
    }

    #[test]
    fn cloned_ledgers_admit_only_one_concurrent_last_budget_request() {
        let service = Arc::new(UsageBudgetService::default());
        let session = Arc::new(sample_session(
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "0.01",
            "0.00",
        ));
        let envelope = Arc::new(envelope(3_000, 1_000));
        let model = Arc::new(model("0.000002", "0.000004", 4_096));
        let handles = (0..2)
            .map(|_| {
                let service = Arc::clone(&service);
                let session = Arc::clone(&session);
                let envelope = Arc::clone(&envelope);
                let model = Arc::clone(&model);
                thread::spawn(move || {
                    service.reserve(RequestId::new(), &session, &envelope, &model)
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .map(|error| error.code)
                .collect::<Vec<_>>(),
            ["usage_session_limit_exceeded"]
        );
    }

    #[test]
    fn envelope_model_snapshot_and_capacity_invariants_are_bounded() {
        let service = UsageBudgetService::default();
        let session = sample_session("cccccccc-cccc-4ccc-8ccc-cccccccccccc", "1.00", "0.00");
        let envelope = envelope(100, 10);
        let mut wrong_model = model("0", "0", 4_096);
        wrong_model.id = "other/model".to_owned();
        assert_eq!(
            service
                .reserve(RequestId::new(), &session, &envelope, &wrong_model)
                .unwrap_err()
                .code,
            "usage_model_mismatch"
        );
        let small_model = model("0", "0", 100);
        assert_eq!(
            service
                .reserve(RequestId::new(), &session, &envelope, &small_model)
                .unwrap_err()
                .code,
            "usage_model_context_too_small"
        );

        let free = model("0", "0", 4_096);
        for _ in 0..MAX_PENDING_RESERVATIONS_PER_SESSION {
            service
                .reserve(RequestId::new(), &session, &envelope, &free)
                .unwrap();
        }
        assert_eq!(
            service
                .reserve(RequestId::new(), &session, &envelope, &free)
                .unwrap_err()
                .code,
            "usage_session_capacity_reached"
        );

        let changed = sample_session("cccccccc-cccc-4ccc-8ccc-cccccccccccc", "2.00", "0.00");
        assert_eq!(
            service
                .reserve(RequestId::new(), &changed, &envelope, &free)
                .unwrap_err()
                .code,
            "usage_session_snapshot_changed"
        );
    }

    #[test]
    fn openrouter_service_reserves_only_from_its_cached_catalog() {
        let router = OpenRouterService::open(CredentialService::in_memory(None)).unwrap();
        let session = sample_session("cccccccc-cccc-4ccc-8ccc-cccccccccccc", "0.01", "0.00");
        let envelope = envelope(1_000, 1_000);
        let request_id = RequestId::new();
        assert_eq!(
            router
                .reserve_prompt_usage(request_id, &session, &envelope)
                .unwrap_err()
                .code,
            "usage_catalog_required"
        );

        router.seed_catalog_for_test(vec![model("0.000001", "0.000002", 4_096)]);
        let reservation = router
            .reserve_prompt_usage(request_id, &session, &envelope)
            .unwrap();
        assert_eq!(reservation.reserved_cost_usd, "0.003000000000");
        let reconciliation = router
            .reconcile_prompt_usage(FinalUsage {
                request_id,
                session_id: session.id,
                input_tokens: 100,
                output_tokens: 50,
            })
            .unwrap();
        assert_eq!(reconciliation.actual_cost_usd, "0.000200000000");
    }
}
