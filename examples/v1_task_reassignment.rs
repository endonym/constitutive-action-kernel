//! A task-board demonstration of the v1 action API.
//!
//! An agent proposes an exact project-task reassignment. The host supplies two
//! attributed condition assessments, evidence references and a response attributed
//! to Bob for the exact offer; only the board adapter changes the task. This
//! in-memory demonstration host supplies assertions; deployed hosts authenticate them.

#[cfg(test)]
use constitutive_action::v1::RevocationRequest;
use constitutive_action::v1::{
    AcceptanceRecord, AcceptanceRequirement, AcceptanceState, ActionIntent, ActionReceipt,
    Admission, AuthorityError, CheckResult, ConditionAssessment, ConditionPolicy,
    ConditionRequirement, DecisionKind, Effect, EffectiveRevocation, EvaluationSnapshot, Evidence,
    ExecutionOutcome, GateDecision, GateProfile, LiveAuthorityState, RevocationState,
    SCHEMA_VERSION, VerificationStatus, evaluate, sha256_hex, validate_receipt,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Serialize)]
struct Reassignment {
    proposal_id: String,
    project_id: String,
    task_id: String,
    agent_id: String,
    sponsor_id: String,
    from: String,
    to: String,
    expected_revision: u64,
}

fn append_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
    bytes.extend_from_slice(field);
}

impl Reassignment {
    /// This profile's canonical bytes are part of its contract. The adapter
    /// reconstructs them from typed fields instead of executing arbitrary text.
    fn canonical_effect(&self) -> Vec<u8> {
        let mut bytes = b"project.task.reassign/v1".to_vec();
        append_field(&mut bytes, self.task_id.as_bytes());
        append_field(&mut bytes, self.from.as_bytes());
        append_field(&mut bytes, self.to.as_bytes());
        bytes.extend_from_slice(&self.expected_revision.to_be_bytes());
        bytes
    }

    fn intent(&self, context_version: u64) -> ActionIntent {
        ActionIntent {
            schema_version: SCHEMA_VERSION,
            proposal_id: self.proposal_id.clone(),
            actor_id: self.agent_id.clone(),
            represented_principal_id: self.sponsor_id.clone(),
            authority_id: "alice-task-delegation".into(),
            context_id: self.project_id.clone(),
            context_version,
            effect: Effect {
                profile_id: "project.task.reassign".into(),
                profile_version: 1,
                operation: "reassign".into(),
                destination: format!("task:{}", self.task_id),
                canonical_payload: self.canonical_effect(),
            },
        }
    }

    /// Include the offered policy version in the acceptance terms. A changed
    /// offer needs a new response even if its task fields happen to be equal.
    fn terms_digest(&self, gate_profile_version: u32) -> String {
        let mut terms = b"task-reassignment-offer/v1".to_vec();
        append_field(&mut terms, &self.canonical_effect());
        terms.extend_from_slice(&gate_profile_version.to_be_bytes());
        sha256_hex(&terms)
    }
}

struct HostState {
    project_id: String,
    task_id: String,
    assignee: String,
    task_revision: u64,
    context_version: u64,
    sequence: u64,
    gate_profile_version: u32,
    delegation_active: bool,
    acceptance: Option<AcceptanceRecord>,
    revocation: RevocationState,
}

impl HostState {
    fn profile(&self, proposal: &Reassignment) -> GateProfile {
        GateProfile {
            profile_id: "project-task-reassignment".into(),
            version: self.gate_profile_version,
            effect_profile_id: "project.task.reassign".into(),
            effect_profile_version: 1,
            conditions: ConditionPolicy::Require(vec![
                ConditionRequirement {
                    condition_id: "sponsor-delegation".into(),
                    version: 1,
                },
                ConditionRequirement {
                    condition_id: "task-current".into(),
                    version: 1,
                },
            ]),
            required_acceptances: vec![AcceptanceRequirement {
                requirement_id: "new-assignee-acceptance".into(),
                principal_id: proposal.to.clone(),
                terms_digest: proposal.terms_digest(self.gate_profile_version),
            }],
        }
    }

    fn evidence(&self, action: &ActionIntent, id: &str, subject: &str, contents: &str) -> Evidence {
        Evidence {
            evidence_id: id.into(),
            kind: "host-state-witness".into(),
            schema_version: 1,
            subject_id: subject.into(),
            issuer_id: "example-host".into(),
            content_sha256: sha256_hex(contents.as_bytes()),
            verifier_id: Some("example-host-verifier".into()),
            verification: VerificationStatus::Verified,
            observed_at_sequence: self.sequence,
            valid_through_sequence: self.sequence,
            action_fingerprint: Some(action.fingerprint()),
        }
    }

    fn snapshot(&self, proposal: &Reassignment) -> (ActionIntent, EvaluationSnapshot) {
        let action = proposal.intent(self.context_version);
        let delegated = self.delegation_active
            && proposal.agent_id == "agent:Alice"
            && proposal.sponsor_id == "Alice";
        let task_current = proposal.project_id == self.project_id
            && proposal.task_id == self.task_id
            && proposal.expected_revision == self.task_revision
            && proposal.from == self.assignee
            && proposal.from != proposal.to;
        let evidence = vec![
            self.evidence(
                &action,
                "delegation-witness",
                &proposal.sponsor_id,
                if delegated { "active" } else { "inactive" },
            ),
            self.evidence(
                &action,
                "task-witness",
                &self.task_id,
                &format!("{}:revision:{}", self.assignee, self.task_revision),
            ),
        ];
        let assessments = vec![
            ConditionAssessment {
                condition_id: "sponsor-delegation".into(),
                version: 1,
                assessor_id: "example-host".into(),
                result: if delegated {
                    CheckResult::Pass
                } else {
                    CheckResult::Fail
                },
                reason: "the agent needs Alice's active task delegation".into(),
                evidence_ids: vec!["delegation-witness".into()],
            },
            ConditionAssessment {
                condition_id: "task-current".into(),
                version: 1,
                assessor_id: "example-host".into(),
                result: if task_current {
                    CheckResult::Pass
                } else {
                    CheckResult::Fail
                },
                reason: "the task and expected revision must still match".into(),
                evidence_ids: vec!["task-witness".into()],
            },
        ];
        (
            action,
            EvaluationSnapshot {
                schema_version: SCHEMA_VERSION,
                context_id: self.project_id.clone(),
                context_version: self.context_version,
                sequence: self.sequence,
                valid_through_sequence: self.sequence,
                profile: self.profile(proposal),
                evidence,
                assessments,
                acceptances: self.acceptance.clone().into_iter().collect(),
                revocation: self.revocation.clone(),
            },
        )
    }

    fn live(&self, proposal: &Reassignment) -> LiveAuthorityState {
        LiveAuthorityState {
            schema_version: SCHEMA_VERSION,
            authority_id: "alice-task-delegation".into(),
            context_id: self.project_id.clone(),
            context_version: self.context_version,
            profile_fingerprint: self.profile(proposal).fingerprint(),
            sequence: self.sequence,
            revocation: self.revocation.clone(),
        }
    }

    /// The demonstration identifies Bob by name; a deployed host authenticates
    /// Bob's identity and authority before storing this decision.
    fn record_acceptance(&mut self, asserted_principal: &str, proposal: &Reassignment) -> bool {
        if asserted_principal != proposal.to
            || proposal.project_id != self.project_id
            || proposal.task_id != self.task_id
            || proposal.expected_revision != self.task_revision
            || proposal.from != self.assignee
        {
            return false;
        }
        self.sequence += 1;
        self.acceptance = Some(AcceptanceRecord {
            requirement_id: "new-assignee-acceptance".into(),
            state: AcceptanceState::Accepted {
                by_principal_id: asserted_principal.into(),
                action_fingerprint: proposal.intent(self.context_version).fingerprint(),
                terms_digest: proposal.terms_digest(self.gate_profile_version),
                at_sequence: self.sequence,
                valid_through_sequence: self.sequence + 5,
            },
        });
        true
    }

    #[cfg(test)]
    fn route_revocation_request(&mut self) {
        self.sequence += 1;
        self.revocation.pending_requests.push(RevocationRequest {
            request_id: "request-1".into(),
            target_authority_id: "alice-task-delegation".into(),
            requested_by: "Alice".into(),
            routed_to: "project-owner".into(),
            at_sequence: self.sequence,
        });
    }

    /// Represents an effective decision, not merely receipt of the request.
    fn make_revocation_effective(&mut self) {
        self.sequence += 1;
        self.delegation_active = false;
        self.revocation.effective = Some(EffectiveRevocation {
            decision_id: "decision-1".into(),
            target_authority_id: "alice-task-delegation".into(),
            effective_at_sequence: self.sequence,
        });
    }
}

struct BoardAdapter {
    host: HostState,
    /// Same-process replay map; a deployed adapter uses durable state.
    receipts: HashMap<String, ActionReceipt>,
}

impl BoardAdapter {
    fn admit(&self, proposal: &Reassignment) -> Admission {
        let (action, snapshot) = self.host.snapshot(proposal);
        evaluate(&action, &snapshot)
    }

    fn apply(
        &mut self,
        proposal: &Reassignment,
        admission: &Admission,
    ) -> Result<ActionReceipt, &'static str> {
        let action = proposal.intent(self.host.context_version);
        if let Some(previous) = self.receipts.get(&proposal.proposal_id) {
            return if previous.action_fingerprint == action.fingerprint() {
                Ok(previous.clone())
            } else {
                Err("proposal ID reused for a different effect")
            };
        }
        let (fresh_action, fresh_snapshot) = self.host.snapshot(proposal);
        let fresh = evaluate(&fresh_action, &fresh_snapshot);
        if admission.decision != fresh.decision {
            return Err("the admission is not current under host-owned state");
        }
        if admission.decision.kind != DecisionKind::Permitted {
            let receipt = ActionReceipt {
                schema_version: SCHEMA_VERSION,
                proposal_id: action.proposal_id.clone(),
                action_fingerprint: action.fingerprint(),
                decision: admission.decision.clone(),
                executor_id: None,
                attempt_id: None,
                outcome: ExecutionOutcome::NotDispatched,
            };
            validate_receipt(&action, admission, &receipt).map_err(|_| "invalid receipt")?;
            return Ok(receipt);
        }
        let authority = admission.authority.as_ref().ok_or("missing authority")?;
        authority
            .check_pre_dispatch(&action, &self.host.live(proposal))
            .map_err(|error| match error {
                AuthorityError::DifferentAction => "altered effect",
                AuthorityError::StaleOrInvalidState => "stale context or profile",
                AuthorityError::EffectivelyRevoked => "effective revocation",
            })?;

        // Reconstruct the exact effect and compare it to the admitted bytes.
        if action.effect.operation != "reassign"
            || action.effect.profile_id != "project.task.reassign"
            || action.effect.profile_version != 1
            || action.effect.destination != format!("task:{}", proposal.task_id)
            || action.effect.canonical_payload != proposal.canonical_effect()
        {
            return Err("effect does not match the adapter profile");
        }
        self.host.assignee = proposal.to.clone();
        self.host.task_revision += 1;
        self.host.sequence += 1;
        let receipt = ActionReceipt {
            schema_version: SCHEMA_VERSION,
            proposal_id: action.proposal_id.clone(),
            action_fingerprint: action.fingerprint(),
            decision: admission.decision.clone(),
            executor_id: Some("example-task-board-adapter".into()),
            attempt_id: Some(format!("attempt:{}", proposal.proposal_id)),
            outcome: ExecutionOutcome::Confirmed {
                occurrence_id: format!(
                    "board:{}:{}:revision:{}",
                    self.host.project_id, self.host.task_id, self.host.task_revision
                ),
            },
        };
        validate_receipt(&action, admission, &receipt).map_err(|_| "invalid receipt")?;
        self.receipts
            .insert(proposal.proposal_id.clone(), receipt.clone());
        Ok(receipt)
    }
}

fn example() -> (BoardAdapter, Reassignment) {
    (
        BoardAdapter {
            host: HostState {
                project_id: "project:greenhouse".into(),
                task_id: "task-7".into(),
                assignee: "Alice".into(),
                task_revision: 3,
                context_version: 1,
                sequence: 1,
                gate_profile_version: 1,
                delegation_active: true,
                acceptance: None,
                revocation: RevocationState::default(),
            },
            receipts: HashMap::new(),
        },
        Reassignment {
            proposal_id: "proposal-42".into(),
            project_id: "project:greenhouse".into(),
            task_id: "task-7".into(),
            agent_id: "agent:Alice".into(),
            sponsor_id: "Alice".into(),
            from: "Alice".into(),
            to: "Bob".into(),
            expected_revision: 3,
        },
    )
}

/// The website's annotated walkthrough is exported from these Rust kernel and
/// adapter calls.
#[derive(Clone, Serialize)]
struct BoardState {
    assignee: String,
    revision: u64,
}

impl BoardState {
    fn read(board: &BoardAdapter) -> Self {
        Self {
            assignee: board.host.assignee.clone(),
            revision: board.host.task_revision,
        }
    }
}

#[derive(Serialize)]
struct AdapterRecord {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submitted_decision: Option<GateDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<ActionReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rejected_previous_admission: Option<&'static str>,
}

#[derive(Serialize)]
struct DemoStep {
    id: &'static str,
    title: &'static str,
    explanation: &'static str,
    scenario: &'static str,
    proposal: Reassignment,
    board_before: BoardState,
    board_after: BoardState,
    decision: GateDecision,
    adapter: AdapterRecord,
}

#[derive(Serialize)]
struct DemoTranscript {
    schema_version: u16,
    crate_version: &'static str,
    wire_version: u16,
    title: &'static str,
    host_kind: &'static str,
    source: &'static str,
    command: &'static str,
    steps: Vec<DemoStep>,
}

fn demonstration() -> DemoTranscript {
    let (mut board, proposal) = example();
    let mut steps = Vec::new();

    let before = BoardState::read(&board);
    let pending = board.admit(&proposal);
    assert_eq!(pending.decision.kind, DecisionKind::DecisionRequired);
    let pending_receipt = board.apply(&proposal, &pending).unwrap();
    assert_eq!(pending_receipt.outcome, ExecutionOutcome::NotDispatched);
    steps.push(DemoStep {
        id: "missing-acceptance",
        title: "Before Bob accepts",
        explanation: "Alice's agent proposes moving task-7 from Alice to Bob at revision 3. The host's profile requires Bob's acceptance of this exact offer. The kernel returns DecisionRequired, and the adapter leaves the board unchanged.",
        scenario: "main",
        proposal: proposal.clone(),
        board_before: before,
        board_after: BoardState::read(&board),
        decision: pending.decision.clone(),
        adapter: AdapterRecord {
            status: "not_dispatched", error: None,
            submitted_decision: Some(pending.decision), receipt: Some(pending_receipt),
            rejected_previous_admission: None,
        },
    });

    let before = BoardState::read(&board);
    assert!(board.host.record_acceptance("Bob", &proposal));
    let admitted = board.admit(&proposal);
    assert_eq!(admitted.decision.kind, DecisionKind::Permitted);
    steps.push(DemoStep {
        id: "exact-acceptance",
        title: "Acceptance attributed to Bob",
        explanation: "The demonstration host records a response attributed to Bob, bound to the proposal's action fingerprint and terms digest. The gate now returns Permitted. Permission has not yet changed the board: execution is a separate adapter step.",
        scenario: "main",
        proposal: proposal.clone(),
        board_before: before,
        board_after: BoardState::read(&board),
        decision: admitted.decision.clone(),
        adapter: AdapterRecord {
            status: "not_attempted", error: None, submitted_decision: None, receipt: None,
            rejected_previous_admission: None,
        },
    });

    let before = BoardState::read(&board);
    let mut changed = proposal.clone();
    changed.to = "Carol".into();
    let changed_admission = board.admit(&changed);
    assert_eq!(
        changed_admission.decision.kind,
        DecisionKind::DecisionRequired
    );
    let changed_error = board.apply(&changed, &admitted).unwrap_err();
    assert_eq!(
        changed_error,
        "the admission is not current under host-owned state"
    );
    steps.push(DemoStep {
        id: "changed-effect",
        title: "The proposal substitutes Carol",
        explanation: "Changing the assignee changes the action and the required acceptance. A fresh gate decision is DecisionRequired. Trying to dispatch Carol's assignment with the earlier admission attributed to Bob is rejected by the host adapter. That response cannot authorise Carol's assignment.",
        scenario: "main",
        proposal: changed,
        board_before: before,
        board_after: BoardState::read(&board),
        decision: changed_admission.decision,
        adapter: AdapterRecord {
            status: "rejected", error: Some(changed_error),
            submitted_decision: Some(admitted.decision.clone()), receipt: None,
            rejected_previous_admission: None,
        },
    });

    let before = BoardState::read(&board);
    let receipt = board.apply(&proposal, &admitted).unwrap();
    assert!(matches!(
        receipt.outcome,
        ExecutionOutcome::Confirmed { .. }
    ));
    assert_eq!(board.host.assignee, "Bob");
    assert_eq!(board.host.task_revision, 4);
    steps.push(DemoStep {
        id: "executed",
        title: "The adapter applies Bob's assignment",
        explanation: "The agent returns to the unchanged Bob proposal. The adapter re-evaluates host-owned state, checks the admitted action, then changes the board once. Its receipt reports the occurrence at revision 4. The kernel's permission and the adapter's outcome remain distinct records.",
        scenario: "main",
        proposal: proposal.clone(),
        board_before: before,
        board_after: BoardState::read(&board),
        decision: admitted.decision.clone(),
        adapter: AdapterRecord {
            status: "confirmed", error: None,
            submitted_decision: Some(admitted.decision.clone()), receipt: Some(receipt.clone()),
            rejected_previous_admission: None,
        },
    });

    let before = BoardState::read(&board);
    let replay = board.apply(&proposal, &admitted).unwrap();
    assert_eq!(receipt, replay);
    assert_eq!(board.host.task_revision, 4);
    assert_eq!(board.receipts.len(), 1);
    steps.push(DemoStep {
        id: "replay",
        title: "A retry returns the same occurrence",
        explanation: "A retry with the same proposal returns the saved receipt and its original Permitted decision; no fresh permission is issued. The board stays at revision 4. The example host's in-memory replay map returns the saved receipt without changing the board again.",
        scenario: "main",
        proposal,
        board_before: before,
        board_after: BoardState::read(&board),
        decision: admitted.decision.clone(),
        adapter: AdapterRecord {
            status: "confirmed", error: None,
            submitted_decision: Some(admitted.decision), receipt: Some(replay),
            rejected_previous_admission: None,
        },
    });

    // A separate run starts from Alice at revision 3: revocation occurs before
    // any dispatch. Revoking authority does not undo the preceding occurrence.
    let (mut revoked_board, revoked_proposal) = example();
    assert!(
        revoked_board
            .host
            .record_acceptance("Bob", &revoked_proposal)
    );
    let old_admission = revoked_board.admit(&revoked_proposal);
    assert_eq!(old_admission.decision.kind, DecisionKind::Permitted);
    let before = BoardState::read(&revoked_board);
    revoked_board.host.make_revocation_effective();
    let revoked = revoked_board.admit(&revoked_proposal);
    assert_eq!(revoked.decision.kind, DecisionKind::Prevented);
    let old_error = revoked_board
        .apply(&revoked_proposal, &old_admission)
        .unwrap_err();
    let revoked_receipt = revoked_board.apply(&revoked_proposal, &revoked).unwrap();
    assert_eq!(revoked_receipt.outcome, ExecutionOutcome::NotDispatched);
    assert_eq!(revoked_board.host.assignee, "Alice");
    assert_eq!(revoked_board.host.task_revision, 3);
    steps.push(DemoStep {
        id: "revoked",
        title: "Effective withdrawal prevents dispatch",
        explanation: "Separate run: start again with Alice at revision 3 and an acceptance attributed to Bob for the exact terms, then make Alice's delegation revocation effective before dispatch. The fresh gate is Prevented, the earlier admission is rejected, and the fresh receipt is NotDispatched. This does not undo the confirmed occurrence in the first run.",
        scenario: "revocation",
        proposal: revoked_proposal,
        board_before: before,
        board_after: BoardState::read(&revoked_board),
        decision: revoked.decision.clone(),
        adapter: AdapterRecord {
            status: "not_dispatched", error: None,
            submitted_decision: Some(revoked.decision), receipt: Some(revoked_receipt),
            rejected_previous_admission: Some(old_error),
        },
    });

    DemoTranscript {
        schema_version: 1,
        crate_version: env!("CARGO_PKG_VERSION"),
        wire_version: SCHEMA_VERSION,
        title: "Keep permission attached to the exact action",
        host_kind: "in-memory demonstration host",
        source: "examples/v1_task_reassignment.rs",
        command: "cargo run --locked --example v1_task_reassignment -- --demo-json",
        steps,
    }
}

fn main() {
    let transcript = demonstration();
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments == ["--demo-json"] {
        println!("{}", serde_json::to_string_pretty(&transcript).unwrap());
        return;
    }
    if !arguments.is_empty() {
        eprintln!("Usage: cargo run --locked --example v1_task_reassignment [-- --demo-json]");
        std::process::exit(2);
    }

    println!("Exact-action permission: an in-memory demonstration host\n");
    for (index, step) in transcript.steps.iter().enumerate() {
        println!("{}. {}: {:?}", index + 1, step.title, step.decision.kind);
        println!("   {}", step.explanation);
        println!(
            "   Board: {} / revision {} -> {} / revision {}",
            step.board_before.assignee,
            step.board_before.revision,
            step.board_after.assignee,
            step.board_after.revision
        );
        if let Some(error) = step.adapter.error {
            println!("   Adapter rejected the earlier admission: {error}");
        }
        if let Some(receipt) = &step.adapter.receipt {
            println!("   Receipt: {:?}", receipt.outcome);
        }
        println!();
    }
    println!(
        "The demonstration host supplies attributed verification and acceptance, then the adapter executes. Deployed hosts authenticate participants and persist replay protection across restarts."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_acceptance_cannot_reassign() {
        let (mut board, proposal) = example();
        let pending = board.admit(&proposal);
        assert_eq!(pending.decision.kind, DecisionKind::DecisionRequired);
        assert!(pending.authority.is_none());
        let receipt = board.apply(&proposal, &pending).unwrap();
        assert_eq!(receipt.outcome, ExecutionOutcome::NotDispatched);
        assert_eq!(board.host.assignee, "Alice");
    }

    #[test]
    fn exact_acceptance_and_pending_revocation_allow_one_effect() {
        let (mut board, proposal) = example();
        assert!(!board.host.record_acceptance("Mallory", &proposal));
        assert!(board.host.record_acceptance("Bob", &proposal));
        board.host.route_revocation_request();
        let admitted = board.admit(&proposal);
        assert_eq!(admitted.decision.kind, DecisionKind::Permitted);
        let first = board.apply(&proposal, &admitted).unwrap();
        assert!(matches!(first.outcome, ExecutionOutcome::Confirmed { .. }));
        let replay = board.apply(&proposal, &admitted).unwrap();
        assert_eq!(first, replay);
        assert_eq!(board.host.task_revision, 4);
        assert_eq!(board.receipts.len(), 1);
    }

    #[test]
    fn changed_effect_and_terms_require_new_acceptance() {
        let (mut board, proposal) = example();
        assert!(board.host.record_acceptance("Bob", &proposal));
        let admitted = board.admit(&proposal);
        let mut changed = proposal.clone();
        changed.to = "Carol".into();
        assert_eq!(
            board.admit(&changed).decision.kind,
            DecisionKind::DecisionRequired
        );
        assert_eq!(
            board.apply(&changed, &admitted),
            Err("the admission is not current under host-owned state")
        );
        assert_eq!(board.host.assignee, "Alice");
    }

    #[test]
    fn effective_revocation_prevents_new_admission_and_old_dispatch() {
        let (mut board, proposal) = example();
        assert!(board.host.record_acceptance("Bob", &proposal));
        let admitted = board.admit(&proposal);
        board.host.make_revocation_effective();
        assert_eq!(
            board.admit(&proposal).decision.kind,
            DecisionKind::Prevented
        );
        assert!(board.apply(&proposal, &admitted).is_err());
        assert_eq!(board.host.assignee, "Alice");
    }

    #[test]
    fn profile_drift_blocks_old_authority_and_invalidates_terms() {
        let (mut board, proposal) = example();
        assert!(board.host.record_acceptance("Bob", &proposal));
        let admitted = board.admit(&proposal);
        let old_profile_fingerprint = admitted.decision.profile_fingerprint.clone();
        board.host.gate_profile_version = 2;
        assert_ne!(
            board.host.profile(&proposal).fingerprint(),
            old_profile_fingerprint
        );
        let old_authority = admitted.authority.as_ref().unwrap();
        assert_eq!(
            old_authority.check_pre_dispatch(&proposal.intent(1), &board.host.live(&proposal)),
            Err(AuthorityError::StaleOrInvalidState)
        );
        assert_eq!(
            board.admit(&proposal).decision.kind,
            DecisionKind::DecisionRequired
        );
        assert!(board.apply(&proposal, &admitted).is_err());
        assert_eq!(board.host.assignee, "Alice");
    }
}
