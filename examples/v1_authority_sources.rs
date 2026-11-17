//! Two sources of authority over the same task board.
//!
//! Charlie manages the board and signs every proposal in this example. The two
//! runs differ only in the host's gate profile, which records where an
//! assignment's authority comes from:
//!
//! - **Manager control.** The board's rules are fused with Charlie's control of
//!   the board. His signature alone moves a task from Bob to Carol and later
//!   changes its deadline. Carol is committed by those changes; she does not
//!   commit to them.
//! - **Commitment.** An assignment rests on the assignee's commitment to its
//!   exact terms. Charlie's signed proposal is held until Carol accepts those
//!   terms, and a later deadline change binds her only if she accepts the
//!   variation. If she refuses, the terms she accepted stand.
//!
//! The kernel API calls a commitment an *acceptance*: a response attributed to
//! the required principal and bound to the exact action and terms digest. This
//! in-memory demonstration host asserts signatures and responses by name;
//! deployed hosts authenticate them.

use constitutive_action::v1::{
    AcceptanceRecord, AcceptanceRequirement, AcceptanceState, ActionIntent, ActionReceipt,
    Admission, AuthorityError, CheckResult, ConditionAssessment, ConditionPolicy,
    ConditionRequirement, DecisionKind, Effect, EvaluationSnapshot, Evidence, ExecutionOutcome,
    GateDecision, GateProfile, LiveAuthorityState, RevocationState, SCHEMA_VERSION,
    VerificationStatus, evaluate, sha256_hex, validate_receipt,
};
use serde::Serialize;
use std::collections::HashMap;

const MANAGER: &str = "Charlie";
const AUTHORITY_ID: &str = "charlie-board-role";
const EFFECT_PROFILE: &str = "project.task.change";
const COMMITMENT_REQUIREMENT: &str = "assignee-commitment";

/// Where the host locates the authority for a change to an assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum AuthoritySource {
    /// Grant-backed: the controller of the board can change assignments alone.
    ManagerControl,
    /// Commitment-backed: the assignee must accept the exact terms they bear.
    Commitment,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Change {
    Reassign { to: String },
    SetDeadline { to: String },
}

#[derive(Clone, Serialize)]
struct Proposal {
    proposal_id: String,
    project_id: String,
    task_id: String,
    /// Asserted signer. A deployed host verifies the signature.
    signed_by: String,
    /// The task state the proposal was written against.
    expected_assignee: String,
    expected_deadline: String,
    expected_revision: u64,
    change: Change,
}

fn append_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
    bytes.extend_from_slice(field);
}

impl Proposal {
    fn operation(&self) -> &'static str {
        match self.change {
            Change::Reassign { .. } => "reassign",
            Change::SetDeadline { .. } => "set-deadline",
        }
    }

    /// The person bound by the task after this change.
    fn bound_assignee(&self) -> &str {
        match &self.change {
            Change::Reassign { to } => to,
            Change::SetDeadline { .. } => &self.expected_assignee,
        }
    }

    /// The deadline that person is bound to after this change.
    fn bound_deadline(&self) -> &str {
        match &self.change {
            Change::Reassign { .. } => &self.expected_deadline,
            Change::SetDeadline { to } => to,
        }
    }

    fn canonical_effect(&self) -> Vec<u8> {
        let mut bytes = b"project.task.change/v1".to_vec();
        append_field(&mut bytes, self.operation().as_bytes());
        append_field(&mut bytes, self.task_id.as_bytes());
        append_field(&mut bytes, self.expected_assignee.as_bytes());
        append_field(&mut bytes, self.expected_deadline.as_bytes());
        bytes.extend_from_slice(&self.expected_revision.to_be_bytes());
        append_field(&mut bytes, self.bound_assignee().as_bytes());
        append_field(&mut bytes, self.bound_deadline().as_bytes());
        bytes
    }

    fn intent(&self, context_version: u64) -> ActionIntent {
        ActionIntent {
            schema_version: SCHEMA_VERSION,
            proposal_id: self.proposal_id.clone(),
            actor_id: self.signed_by.clone(),
            represented_principal_id: self.signed_by.clone(),
            authority_id: AUTHORITY_ID.into(),
            context_id: self.project_id.clone(),
            context_version,
            effect: Effect {
                profile_id: EFFECT_PROFILE.into(),
                profile_version: 1,
                operation: self.operation().into(),
                destination: format!("task:{}", self.task_id),
                canonical_payload: self.canonical_effect(),
            },
        }
    }

    /// The terms the assignee would be committed to: who holds the task and by
    /// when, under the offered profile version.
    fn terms_digest(&self, gate_profile_version: u32) -> String {
        let mut terms = b"task-assignment-terms/v1".to_vec();
        append_field(&mut terms, self.task_id.as_bytes());
        append_field(&mut terms, self.bound_assignee().as_bytes());
        append_field(&mut terms, self.bound_deadline().as_bytes());
        terms.extend_from_slice(&gate_profile_version.to_be_bytes());
        sha256_hex(&terms)
    }
}

struct HostState {
    source: AuthoritySource,
    project_id: String,
    task_id: String,
    assignee: String,
    deadline: String,
    task_revision: u64,
    context_version: u64,
    sequence: u64,
    gate_profile_version: u32,
    /// Responses keyed by proposal: each binds only the proposal it answers.
    responses: HashMap<String, AcceptanceRecord>,
}

impl HostState {
    fn profile(&self, proposal: &Proposal) -> GateProfile {
        let (profile_id, required_acceptances) = match self.source {
            AuthoritySource::ManagerControl => ("task-board-manager-control", Vec::new()),
            AuthoritySource::Commitment => (
                "task-board-assignee-commitment",
                vec![AcceptanceRequirement {
                    requirement_id: COMMITMENT_REQUIREMENT.into(),
                    principal_id: proposal.bound_assignee().into(),
                    terms_digest: proposal.terms_digest(self.gate_profile_version),
                }],
            ),
        };
        GateProfile {
            profile_id: profile_id.into(),
            version: self.gate_profile_version,
            effect_profile_id: EFFECT_PROFILE.into(),
            effect_profile_version: 1,
            conditions: ConditionPolicy::Require(vec![
                ConditionRequirement {
                    condition_id: "manager-signature".into(),
                    version: 1,
                },
                ConditionRequirement {
                    condition_id: "task-current".into(),
                    version: 1,
                },
            ]),
            required_acceptances,
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

    fn task_current(&self, proposal: &Proposal) -> bool {
        proposal.project_id == self.project_id
            && proposal.task_id == self.task_id
            && proposal.expected_revision == self.task_revision
            && proposal.expected_assignee == self.assignee
            && proposal.expected_deadline == self.deadline
            && (proposal.bound_assignee(), proposal.bound_deadline())
                != (self.assignee.as_str(), self.deadline.as_str())
    }

    fn snapshot(&self, proposal: &Proposal) -> (ActionIntent, EvaluationSnapshot) {
        let action = proposal.intent(self.context_version);
        let signed_by_manager = proposal.signed_by == MANAGER;
        let task_current = self.task_current(proposal);
        let result = |pass| {
            if pass {
                CheckResult::Pass
            } else {
                CheckResult::Fail
            }
        };
        let evidence = vec![
            self.evidence(
                &action,
                "signature-witness",
                &proposal.signed_by,
                "signature:verified",
            ),
            self.evidence(
                &action,
                "task-witness",
                &self.task_id,
                &format!(
                    "{}:{}:revision:{}",
                    self.assignee, self.deadline, self.task_revision
                ),
            ),
        ];
        let assessments = vec![
            ConditionAssessment {
                condition_id: "manager-signature".into(),
                version: 1,
                assessor_id: "example-host".into(),
                result: result(signed_by_manager),
                reason: "the proposal must carry the board manager's signature".into(),
                evidence_ids: vec!["signature-witness".into()],
            },
            ConditionAssessment {
                condition_id: "task-current".into(),
                version: 1,
                assessor_id: "example-host".into(),
                result: result(task_current),
                reason: "the task, assignee, deadline and revision must still match".into(),
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
                acceptances: self
                    .responses
                    .get(&proposal.proposal_id)
                    .cloned()
                    .into_iter()
                    .collect(),
                revocation: RevocationState::default(),
            },
        )
    }

    fn live(&self, proposal: &Proposal) -> LiveAuthorityState {
        LiveAuthorityState {
            schema_version: SCHEMA_VERSION,
            authority_id: AUTHORITY_ID.into(),
            context_id: self.project_id.clone(),
            context_version: self.context_version,
            profile_fingerprint: self.profile(proposal).fingerprint(),
            sequence: self.sequence,
            revocation: RevocationState::default(),
        }
    }

    /// Record the bound person's response to the exact terms. Under manager
    /// control the profile has no place for it, so the host does not record it:
    /// the person's agreement is not an input to the decision.
    fn record_response(
        &mut self,
        asserted_principal: &str,
        proposal: &Proposal,
        accept: bool,
    ) -> bool {
        if self.source != AuthoritySource::Commitment
            || asserted_principal != proposal.bound_assignee()
            || !self.task_current(proposal)
        {
            return false;
        }
        self.sequence += 1;
        let by_principal_id = asserted_principal.to_string();
        let action_fingerprint = proposal.intent(self.context_version).fingerprint();
        let terms_digest = proposal.terms_digest(self.gate_profile_version);
        let at_sequence = self.sequence;
        let state = if accept {
            AcceptanceState::Accepted {
                by_principal_id,
                action_fingerprint,
                terms_digest,
                at_sequence,
                valid_through_sequence: at_sequence + 5,
            }
        } else {
            AcceptanceState::Refused {
                by_principal_id,
                action_fingerprint,
                terms_digest,
                at_sequence,
            }
        };
        self.responses.insert(
            proposal.proposal_id.clone(),
            AcceptanceRecord {
                requirement_id: COMMITMENT_REQUIREMENT.into(),
                state,
            },
        );
        true
    }
}

struct BoardAdapter {
    host: HostState,
    /// Same-process replay map; a deployed adapter uses durable state.
    receipts: HashMap<String, ActionReceipt>,
}

impl BoardAdapter {
    fn admit(&self, proposal: &Proposal) -> Admission {
        let (action, snapshot) = self.host.snapshot(proposal);
        evaluate(&action, &snapshot)
    }

    fn apply(
        &mut self,
        proposal: &Proposal,
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
        if admission.decision != self.admit(proposal).decision {
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
        if action.effect.profile_id != EFFECT_PROFILE
            || action.effect.destination != format!("task:{}", proposal.task_id)
            || action.effect.canonical_payload != proposal.canonical_effect()
        {
            return Err("effect does not match the adapter profile");
        }
        self.host.assignee = proposal.bound_assignee().into();
        self.host.deadline = proposal.bound_deadline().into();
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

    /// Charlie's signed proposal against the board's current state.
    fn propose(&self, proposal_id: &str, change: Change) -> Proposal {
        Proposal {
            proposal_id: proposal_id.into(),
            project_id: self.host.project_id.clone(),
            task_id: self.host.task_id.clone(),
            signed_by: MANAGER.into(),
            expected_assignee: self.host.assignee.clone(),
            expected_deadline: self.host.deadline.clone(),
            expected_revision: self.host.task_revision,
            change,
        }
    }
}

/// Bob holds task-7 at revision 4, due 2026-12-01: where the first example ends.
fn board(source: AuthoritySource) -> BoardAdapter {
    BoardAdapter {
        host: HostState {
            source,
            project_id: "project:greenhouse".into(),
            task_id: "task-7".into(),
            assignee: "Bob".into(),
            deadline: "2026-12-01".into(),
            task_revision: 4,
            context_version: 1,
            sequence: 1,
            gate_profile_version: 1,
            responses: HashMap::new(),
        },
        receipts: HashMap::new(),
    }
}

fn move_to_carol() -> Change {
    Change::Reassign { to: "Carol".into() }
}

fn deadline(to: &str) -> Change {
    Change::SetDeadline { to: to.into() }
}

#[derive(Clone, Serialize)]
struct BoardState {
    assignee: String,
    deadline: String,
    revision: u64,
}

impl BoardState {
    fn read(board: &BoardAdapter) -> Self {
        Self {
            assignee: board.host.assignee.clone(),
            deadline: board.host.deadline.clone(),
            revision: board.host.task_revision,
        }
    }
}

#[derive(Serialize)]
struct DemoStep {
    id: &'static str,
    source: AuthoritySource,
    title: &'static str,
    explanation: &'static str,
    proposal: Proposal,
    board_before: BoardState,
    board_after: BoardState,
    decision: GateDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<ActionReceipt>,
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

struct Recorder(Vec<DemoStep>);

impl Recorder {
    /// Optionally record a response, then evaluate the proposal and pass the
    /// fresh admission to the adapter, recording what the board did.
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        board: &mut BoardAdapter,
        proposal: &Proposal,
        response: Option<(&str, bool)>,
        expected: DecisionKind,
        id: &'static str,
        title: &'static str,
        explanation: &'static str,
    ) {
        let before = BoardState::read(board);
        if let Some((principal, accept)) = response {
            assert!(board.host.record_response(principal, proposal, accept));
        }
        let admission = board.admit(proposal);
        assert_eq!(admission.decision.kind, expected, "{id}");
        let receipt = board.apply(proposal, &admission).unwrap();
        assert_eq!(
            matches!(receipt.outcome, ExecutionOutcome::Confirmed { .. }),
            expected == DecisionKind::Permitted,
            "{id}"
        );
        self.0.push(DemoStep {
            id,
            source: board.host.source,
            title,
            explanation,
            proposal: proposal.clone(),
            board_before: before,
            board_after: BoardState::read(board),
            decision: admission.decision,
            receipt: Some(receipt),
        });
    }
}

fn demonstration() -> DemoTranscript {
    use AuthoritySource::*;
    use DecisionKind::*;
    let mut steps = Recorder(Vec::new());

    // Run A: the board's rules are fused with Charlie's control of the board.
    let mut controlled = board(ManagerControl);
    let move_a = controlled.propose("control-move", move_to_carol());
    steps.step(
        &mut controlled, &move_a, None, Permitted,
        "control-move",
        "Manager control: Charlie moves task-7 to Carol",
        "The profile requires the manager's signature and a current task, and no response from anyone else. Charlie's signature alone permits the move, and the adapter assigns Carol at revision 5. Carol is committed to task-7 without having committed to it.",
    );
    assert_eq!(controlled.host.assignee, "Carol");
    let deadline_a = controlled.propose("control-deadline", deadline("2026-11-24"));
    steps.step(
        &mut controlled, &deadline_a, None, Permitted,
        "control-deadline",
        "Manager control: Charlie brings the deadline forward",
        "Charlie signs a change of deadline from 2026-12-01 to 2026-11-24. It takes effect on his signature alone, at revision 6. The profile has no place for Carol's agreement, so the host would not record a refusal from her.",
    );
    assert!(!controlled.host.record_response("Carol", &deadline_a, false));
    assert_eq!(controlled.host.deadline, "2026-11-24");

    // Run B: assignments rest on the assignee's commitment to exact terms.
    let mut committed = board(Commitment);
    let move_b = committed.propose("commitment-move", move_to_carol());
    steps.step(
        &mut committed, &move_b, None, DecisionRequired,
        "commitment-move-held",
        "Commitment: the same signed move is held",
        "Separate run from the same starting board. Charlie signs the same move, and the signature and task checks pass. The profile also requires Carol's acceptance of the terms she would bear: task-7, due 2026-12-01. Without it the decision is DecisionRequired and the board stays with Bob at revision 4.",
    );
    steps.step(
        &mut committed, &move_b, Some(("Carol", true)), Permitted,
        "commitment-move-accepted",
        "Commitment: Carol accepts the exact terms",
        "The host records Carol's acceptance, bound to this proposal's action fingerprint and terms digest. The gate is Permitted and the adapter assigns Carol at revision 5. She is committed because she committed.",
    );
    let deadline_b = committed.propose("commitment-deadline", deadline("2026-11-24"));
    steps.step(
        &mut committed, &deadline_b, None, DecisionRequired,
        "commitment-deadline-held",
        "Commitment: Charlie's deadline change is held",
        "Charlie signs the same change of deadline. Carol accepted terms due 2026-12-01; that acceptance does not cover new terms due 2026-11-24. The change is DecisionRequired and her deadline stays 2026-12-01.",
    );
    steps.step(
        &mut committed, &deadline_b, Some(("Carol", false)), Prevented,
        "commitment-deadline-refused",
        "Commitment: Carol refuses the variation",
        "Carol refuses the exact variation. The gate is Prevented and the adapter dispatches nothing. Charlie's signature cannot change the terms she accepted; they remain due 2026-12-01 at revision 5.",
    );
    let variation = committed.propose("commitment-variation", deadline("2026-11-28"));
    steps.step(
        &mut committed, &variation, Some(("Carol", true)), Permitted,
        "commitment-variation-accepted",
        "Commitment: Carol accepts a different variation",
        "Charlie signs a new proposal moving the deadline to 2026-11-28, and Carol accepts those exact terms. The gate is Permitted and the adapter records the new deadline at revision 6. New terms apply because Carol committed to the variation.",
    );
    assert_eq!(committed.host.deadline, "2026-11-28");

    DemoTranscript {
        schema_version: 1,
        crate_version: env!("CARGO_PKG_VERSION"),
        wire_version: SCHEMA_VERSION,
        title: "Two sources of authority over the same task",
        host_kind: "in-memory demonstration host",
        source: "examples/v1_authority_sources.rs",
        command: "cargo run --locked --example v1_authority_sources -- --demo-json",
        steps: steps.0,
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
        eprintln!("Usage: cargo run --locked --example v1_authority_sources [-- --demo-json]");
        std::process::exit(2);
    }

    println!("Two sources of authority: an in-memory demonstration host\n");
    for (index, step) in transcript.steps.iter().enumerate() {
        println!("{}. {}: {:?}", index + 1, step.title, step.decision.kind);
        println!("   {}", step.explanation);
        println!(
            "   Board: {} due {} / revision {} -> {} due {} / revision {}",
            step.board_before.assignee,
            step.board_before.deadline,
            step.board_before.revision,
            step.board_after.assignee,
            step.board_after.deadline,
            step.board_after.revision
        );
        if let Some(receipt) = &step.receipt {
            println!("   Receipt: {:?}", receipt.outcome);
        }
        println!();
    }
    println!(
        "Both runs use the same kernel, signer and adapter. Only the host's gate profile differs: whether the person bound by a change must accept its exact terms."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_signature_alone_moves_and_redates_the_task() {
        let mut board = board(AuthoritySource::ManagerControl);
        let moved = board.propose("move", move_to_carol());
        let admission = board.admit(&moved);
        assert_eq!(admission.decision.kind, DecisionKind::Permitted);
        assert!(board.host.profile(&moved).required_acceptances.is_empty());
        assert!(!board.host.record_response("Carol", &moved, false));
        board.apply(&moved, &admission).unwrap();
        let redated = board.propose("redate", deadline("2026-11-24"));
        let admission = board.admit(&redated);
        assert_eq!(admission.decision.kind, DecisionKind::Permitted);
        board.apply(&redated, &admission).unwrap();
        assert_eq!(
            (
                board.host.assignee.as_str(),
                board.host.deadline.as_str(),
                board.host.task_revision
            ),
            ("Carol", "2026-11-24", 6)
        );
    }

    #[test]
    fn manager_control_still_requires_the_managers_signature() {
        let mut board = board(AuthoritySource::ManagerControl);
        let mut forged = board.propose("move", move_to_carol());
        forged.signed_by = "Mallory".into();
        let admission = board.admit(&forged);
        assert_eq!(admission.decision.kind, DecisionKind::Prevented);
        let receipt = board.apply(&forged, &admission).unwrap();
        assert_eq!(receipt.outcome, ExecutionOutcome::NotDispatched);
        assert_eq!(board.host.assignee, "Bob");
    }

    #[test]
    fn commitment_holds_the_move_until_carol_accepts_exact_terms() {
        let mut board = board(AuthoritySource::Commitment);
        let moved = board.propose("move", move_to_carol());
        let held = board.admit(&moved);
        assert_eq!(held.decision.kind, DecisionKind::DecisionRequired);
        assert_eq!(
            board.apply(&moved, &held).unwrap().outcome,
            ExecutionOutcome::NotDispatched
        );
        // Only the person bound by the terms can commit to them.
        assert!(!board.host.record_response("Bob", &moved, true));
        assert!(!board.host.record_response("Charlie", &moved, true));
        assert_eq!(board.host.assignee, "Bob");
        assert!(board.host.record_response("Carol", &moved, true));
        let admitted = board.admit(&moved);
        assert_eq!(admitted.decision.kind, DecisionKind::Permitted);
        assert!(matches!(
            board.apply(&moved, &admitted).unwrap().outcome,
            ExecutionOutcome::Confirmed { .. }
        ));
        assert_eq!(board.host.assignee, "Carol");
    }

    #[test]
    fn acceptance_of_one_proposal_does_not_cover_another() {
        let mut board = board(AuthoritySource::Commitment);
        let moved = board.propose("move", move_to_carol());
        assert!(board.host.record_response("Carol", &moved, true));
        let admitted = board.admit(&moved);
        let mut redated = moved.clone();
        redated.change = deadline("2026-11-24");
        assert_eq!(
            board.admit(&redated).decision.kind,
            DecisionKind::DecisionRequired
        );
        assert_eq!(
            board.apply(&redated, &admitted),
            Err("the admission is not current under host-owned state")
        );
        assert_eq!(board.host.assignee, "Bob");
    }

    #[test]
    fn deadline_change_binds_the_assignee_only_if_accepted() {
        let mut board = board(AuthoritySource::Commitment);
        let moved = board.propose("move", move_to_carol());
        assert!(board.host.record_response("Carol", &moved, true));
        let admitted = board.admit(&moved);
        board.apply(&moved, &admitted).unwrap();

        let redated = board.propose("redate", deadline("2026-11-24"));
        assert_eq!(
            board.admit(&redated).decision.kind,
            DecisionKind::DecisionRequired
        );
        assert!(board.host.record_response("Carol", &redated, false));
        let refused = board.admit(&redated);
        assert_eq!(refused.decision.kind, DecisionKind::Prevented);
        assert_eq!(
            board.apply(&redated, &refused).unwrap().outcome,
            ExecutionOutcome::NotDispatched
        );
        assert_eq!(board.host.deadline, "2026-12-01");

        let variation = board.propose("variation", deadline("2026-11-28"));
        assert!(board.host.record_response("Carol", &variation, true));
        let admitted = board.admit(&variation);
        assert_eq!(admitted.decision.kind, DecisionKind::Permitted);
        board.apply(&variation, &admitted).unwrap();
        assert_eq!(
            (
                board.host.assignee.as_str(),
                board.host.deadline.as_str(),
                board.host.task_revision
            ),
            ("Carol", "2026-11-28", 6)
        );
    }

    #[test]
    fn demonstration_runs_side_by_side() {
        let transcript = demonstration();
        let kinds: Vec<_> = transcript
            .steps
            .iter()
            .map(|step| (step.source, step.decision.kind))
            .collect();
        use AuthoritySource::*;
        use DecisionKind::*;
        assert_eq!(
            kinds,
            [
                (ManagerControl, Permitted),
                (ManagerControl, Permitted),
                (Commitment, DecisionRequired),
                (Commitment, Permitted),
                (Commitment, DecisionRequired),
                (Commitment, Prevented),
                (Commitment, Permitted),
            ]
        );
    }
}
