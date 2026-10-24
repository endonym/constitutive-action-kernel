//! A deliberately in-memory implementation demonstrating that the public probes are
//! executable. It is not evidence of durable host storage or capability
//! isolation. Real hosts should run `check_host` against their own adapter.

use constitutive_action::host_conformance::{
    AttemptState, DispatchTicket, HostHandle, HostScenario, Prepare, Reservation, RevocationCommit,
    check_host,
};
use constitutive_action::v1::{
    ActionIntent, Admission, DecisionKind, Effect, EffectiveRevocation, EvaluationSnapshot,
    LiveAuthorityState, RevocationState, SCHEMA_VERSION, evaluate,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    order: u64,
    revocation: RevocationState,
    record: Option<Record>,
}

struct Record {
    intent: ActionIntent,
    snapshot: EvaluationSnapshot,
    admission: Admission,
    ticket: Option<DispatchTicket>,
    outcome: AttemptState,
}

#[derive(Default)]
struct Scenario(Arc<Mutex<State>>);

struct Handle(Arc<Mutex<State>>);

impl HostScenario for Scenario {
    type Handle = Handle;

    fn open(&self) -> Self::Handle {
        Handle(self.0.clone())
    }
}

impl HostHandle for Handle {
    type Error = &'static str;

    fn prepare(
        &mut self,
        intent: &ActionIntent,
        snapshot: &EvaluationSnapshot,
    ) -> Result<Prepare, Self::Error> {
        let mut state = self.0.lock().map_err(|_| "poisoned state")?;
        if let Some(record) = &state.record {
            return Ok(
                if record.intent == *intent && record.snapshot == *snapshot {
                    Prepare::Existing
                } else {
                    Prepare::Conflict
                },
            );
        }
        let admission = evaluate(intent, snapshot);
        if admission.decision.kind != DecisionKind::Permitted {
            return Err("fixture was not permitted");
        }
        state.order = snapshot.sequence;
        state.record = Some(Record {
            intent: intent.clone(),
            snapshot: snapshot.clone(),
            admission,
            ticket: None,
            outcome: AttemptState::InFlight,
        });
        Ok(Prepare::Stored)
    }

    fn reserve(&mut self, proposal_id: &str) -> Result<Reservation, Self::Error> {
        let mut state = self.0.lock().map_err(|_| "poisoned state")?;
        let Some(record) = &state.record else {
            return Ok(Reservation::Denied);
        };
        if record.intent.proposal_id != proposal_id {
            return Ok(Reservation::Denied);
        }
        if let Some(ticket) = &record.ticket {
            return Ok(Reservation::Replay {
                ticket: ticket.clone(),
                state: record.outcome.clone(),
            });
        }
        let live = LiveAuthorityState {
            schema_version: SCHEMA_VERSION,
            authority_id: record.intent.authority_id.clone(),
            context_id: record.intent.context_id.clone(),
            context_version: record.intent.context_version,
            profile_fingerprint: record.admission.decision.profile_fingerprint.clone(),
            sequence: state.order,
            revocation: state.revocation.clone(),
        };
        if record
            .admission
            .authority
            .as_ref()
            .expect("permitted fixture has authority")
            .check_pre_dispatch(&record.intent, &live)
            .is_err()
        {
            return Ok(Reservation::Denied);
        }
        state.order += 1;
        let reserved_at_order = state.order;
        let record = state.record.as_mut().expect("record checked above");
        let ticket = DispatchTicket {
            proposal_id: record.intent.proposal_id.clone(),
            action_fingerprint: record.intent.fingerprint(),
            attempt_id: format!("attempt:{}", record.intent.proposal_id),
            replay_id: format!("idempotency:{}", record.intent.proposal_id),
            reserved_at_order,
        };
        record.ticket = Some(ticket.clone());
        Ok(Reservation::New(ticket))
    }

    fn effective_revoke(&mut self, authority_id: &str) -> Result<RevocationCommit, Self::Error> {
        let mut state = self.0.lock().map_err(|_| "poisoned state")?;
        let Some(record) = &state.record else {
            return Err("authority missing");
        };
        if record.intent.authority_id != authority_id {
            return Err("different authority");
        }
        state.order += 1;
        state.revocation.effective = Some(EffectiveRevocation {
            decision_id: "host-conformance:effective-decision".into(),
            target_authority_id: authority_id.into(),
            effective_at_sequence: state.order,
        });
        Ok(RevocationCommit { order: state.order })
    }

    fn record_uncertain(&mut self, attempt_id: &str, reason: &str) -> Result<(), Self::Error> {
        let mut state = self.0.lock().map_err(|_| "poisoned state")?;
        let record = state.record.as_mut().ok_or("attempt missing")?;
        if record
            .ticket
            .as_ref()
            .map(|ticket| ticket.attempt_id.as_str())
            != Some(attempt_id)
        {
            return Err("different attempt");
        }
        record.outcome = AttemptState::Uncertain {
            reason: reason.into(),
        };
        Ok(())
    }

    fn adapter_matches(
        &mut self,
        ticket: &DispatchTicket,
        actual_effect: &Effect,
    ) -> Result<bool, Self::Error> {
        let state = self.0.lock().map_err(|_| "poisoned state")?;
        let record = state.record.as_ref().ok_or("admission missing")?;
        Ok(record.ticket.as_ref() == Some(ticket) && record.intent.effect == *actual_effect)
    }
}

#[test]
fn in_memory_reference_exercises_host_probe_contract() {
    check_host(Scenario::default);
}
