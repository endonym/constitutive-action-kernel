//! A project-task reassignment demonstration using the crate-root action API.
//!
//! An agent proposes reassigning a project task. The host resolves delegation,
//! task state and the proposed assignee's acceptance of these exact terms. Only
//! the board adapter can mutate the task. This in-memory example demonstrates
//! the gate and adapter path; deployed hosts supply authenticated acceptance,
//! durable recovery and capability isolation.

use constitutive_action::{
    ActionIntent, ActionReceipt, Admission, Check, CheckResult, DecisionKind, EvaluationContext,
    ExecutionOutcome, LiveState, evaluate,
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct Reassignment {
    proposal_id: String,
    agent_id: String,
    sponsor_id: String,
    project_id: String,
    task_id: String,
    from: String,
    to: String,
    expected_revision: u64,
}

impl Reassignment {
    fn intent(&self, context_version: u64) -> ActionIntent {
        // Length-prefixing keeps even names containing separators unambiguous.
        let mut payload = format!("expected_revision={};", self.expected_revision);
        for value in [&self.from, &self.to] {
            payload.push_str(&format!("{}:", value.len()));
            payload.push_str(value);
        }
        ActionIntent {
            proposal_id: self.proposal_id.clone(),
            actor_id: self.agent_id.clone(),
            represented_participant_id: self.sponsor_id.clone(),
            context_id: self.project_id.clone(),
            context_version,
            operation: "project.task.reassign".into(),
            destination: format!("task:{}", self.task_id),
            payload,
        }
    }
}

struct BoardState {
    project_id: String,
    task_id: String,
    assignee: String,
    task_revision: u64,
    context_version: u64,
    sequence: u64,
    sponsor_id: String,
    delegated_agent_id: String,
    delegation_active: bool,
    accepted_fingerprint: Option<String>,
}

impl BoardState {
    fn live(&self) -> LiveState {
        LiveState {
            context_version: self.context_version,
            sequence: self.sequence,
            revoked: !self.delegation_active,
        }
    }

    fn resolve(&self, proposal: &Reassignment) -> (ActionIntent, EvaluationContext) {
        let intent = proposal.intent(self.context_version);
        let delegation_valid = self.delegation_active
            && proposal.sponsor_id == self.sponsor_id
            && proposal.agent_id == self.delegated_agent_id;
        let task_current = proposal.project_id == self.project_id
            && proposal.task_id == self.task_id
            && proposal.expected_revision == self.task_revision
            && proposal.from == self.assignee
            && proposal.from != proposal.to;
        let acceptance_current =
            self.accepted_fingerprint.as_deref() == Some(intent.fingerprint().as_str());
        let mut evidence_refs = vec![
            format!("delegation:{}:{}", self.sponsor_id, self.delegated_agent_id),
            format!("task:{}:revision:{}", self.task_id, self.task_revision),
        ];
        if acceptance_current {
            evidence_refs.push(format!(
                "acceptance:{}:{}",
                proposal.to,
                intent.fingerprint()
            ));
        }
        let context = EvaluationContext {
            context_id: self.project_id.clone(),
            version: self.context_version,
            sequence: self.sequence,
            valid_through_sequence: self.sequence,
            evidence_refs,
            required_acceptances: if acceptance_current {
                vec![]
            } else {
                vec![proposal.to.clone()]
            },
            checks: vec![
                Check {
                    source: "sponsor delegation".into(),
                    result: if delegation_valid {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail
                    },
                    reason: "this agent needs the sponsor's active delegation".into(),
                },
                Check {
                    source: "task revision and assignee".into(),
                    result: if task_current {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail
                    },
                    reason: "the exact task, current assignee and revision must match".into(),
                },
            ],
        };
        (intent, context)
    }

    /// The demonstration matches Bob's name; a deployed host authenticates
    /// the response and keeps this path outside the proposing agent's reach.
    fn accept(&mut self, participant_id: &str, proposal: &Reassignment) -> bool {
        if participant_id != proposal.to
            || proposal.project_id != self.project_id
            || proposal.task_id != self.task_id
            || proposal.expected_revision != self.task_revision
            || proposal.from != self.assignee
        {
            return false;
        }
        self.accepted_fingerprint = Some(proposal.intent(self.context_version).fingerprint());
        self.sequence += 1;
        true
    }

    /// This represents an effective revocation, not the request/decision process.
    #[cfg(test)]
    fn revoke_delegation(&mut self) {
        self.delegation_active = false;
        self.sequence += 1;
    }
}

/// Holds the demonstration's mutation path. Deployed adapters use a durable
/// transactional claim, authenticated host records and crash-safe reconciliation.
struct BoardAdapter {
    state: BoardState,
    receipts: HashMap<String, ActionReceipt>,
}

impl BoardAdapter {
    fn admit(&self, proposal: &Reassignment) -> Admission {
        let (intent, context) = self.state.resolve(proposal);
        evaluate(&intent, &context)
    }

    fn execute(&mut self, proposal: &Reassignment, admission: &Admission) -> ActionReceipt {
        let intent = proposal.intent(self.state.context_version);
        let mut receipt = ActionReceipt {
            proposal: intent.clone(),
            decision: admission.decision.clone(),
            executor: "example:task-board-adapter".into(),
            transaction_id: None,
            outcome: ExecutionOutcome::NotDispatched,
        };

        // A replay returns the historical result, but an ID cannot name new terms.
        if let Some(previous) = self.receipts.get(&proposal.proposal_id) {
            if previous.proposal == intent {
                return previous.clone();
            }
            receipt.outcome = ExecutionOutcome::Rejected {
                reason: "proposal ID already belongs to different terms".into(),
            };
            return receipt;
        }

        // Re-resolve from host-owned state rather than trusting caller checks.
        let (fresh_intent, fresh_context) = self.state.resolve(proposal);
        let fresh = evaluate(&fresh_intent, &fresh_context);
        if admission.decision != fresh.decision {
            receipt.outcome = ExecutionOutcome::Rejected {
                reason: "admission no longer matches authoritative task state".into(),
            };
            return receipt;
        }
        if admission.decision.kind != DecisionKind::Permitted {
            return receipt;
        }
        let Some(authority) = &admission.authority else {
            receipt.outcome = ExecutionOutcome::Rejected {
                reason: "missing exact-action authority".into(),
            };
            return receipt;
        };
        if authority.check(&intent, self.state.live()).is_err() {
            receipt.outcome = ExecutionOutcome::Rejected {
                reason: "authority is altered, stale or revoked".into(),
            };
            return receipt;
        }

        self.state.assignee = proposal.to.clone();
        self.state.task_revision += 1;
        self.state.sequence += 1;
        receipt.transaction_id = Some(format!(
            "board:{}:{}",
            self.state.project_id, proposal.proposal_id
        ));
        // The v0.1 API calls this external_event_id; here it is an internal
        // task-board occurrence identifier returned by the effect adapter.
        receipt.outcome = ExecutionOutcome::Confirmed {
            external_event_id: format!(
                "task-board:{}:{}:revision:{}",
                self.state.project_id, self.state.task_id, self.state.task_revision
            ),
        };
        self.receipts
            .insert(proposal.proposal_id.clone(), receipt.clone());
        receipt
    }
}

fn example() -> (BoardAdapter, Reassignment) {
    (
        BoardAdapter {
            state: BoardState {
                project_id: "project:greenhouse".into(),
                task_id: "task-7".into(),
                assignee: "Alice".into(),
                task_revision: 3,
                context_version: 1,
                sequence: 1,
                sponsor_id: "Alice".into(),
                delegated_agent_id: "agent:Alice".into(),
                delegation_active: true,
                accepted_fingerprint: None,
            },
            receipts: HashMap::new(),
        },
        Reassignment {
            proposal_id: "proposal:42".into(),
            agent_id: "agent:Alice".into(),
            sponsor_id: "Alice".into(),
            project_id: "project:greenhouse".into(),
            task_id: "task-7".into(),
            from: "Alice".into(),
            to: "Bob".into(),
            expected_revision: 3,
        },
    )
}

fn main() {
    let (mut board, proposal) = example();
    let pending = board.admit(&proposal);
    println!("Before Bob accepts: {:?}", pending.decision.kind);
    assert_eq!(pending.decision.kind, DecisionKind::DecisionRequired);

    assert!(board.state.accept("Bob", &proposal));
    let admitted = board.admit(&proposal);
    println!("After exact-term acceptance: {:?}", admitted.decision.kind);
    let receipt = board.execute(&proposal, &admitted);
    println!("Effect: {:?}", receipt.outcome);
    assert_eq!(board.state.assignee, "Bob");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceptance_is_required_and_no_effect_occurs_while_pending() {
        let (mut board, proposal) = example();
        let pending = board.admit(&proposal);
        assert_eq!(pending.decision.kind, DecisionKind::DecisionRequired);
        assert!(pending.authority.is_none());
        assert_eq!(
            board.execute(&proposal, &pending).outcome,
            ExecutionOutcome::NotDispatched
        );
        assert_eq!(board.state.assignee, "Alice");
    }

    #[test]
    fn exact_terms_execute_once_and_replay_returns_the_same_occurrence() {
        let (mut board, proposal) = example();
        assert!(!board.state.accept("Mallory", &proposal));
        assert!(board.state.accept("Bob", &proposal));
        let admitted = board.admit(&proposal);
        assert_eq!(admitted.decision.kind, DecisionKind::Permitted);
        let first = board.execute(&proposal, &admitted);
        assert!(matches!(first.outcome, ExecutionOutcome::Confirmed { .. }));
        let replay = board.execute(&proposal, &admitted);
        assert_eq!(first, replay);
        assert_eq!(board.state.task_revision, 4);
        assert_eq!(board.receipts.len(), 1);
    }

    #[test]
    fn changed_terms_cannot_borrow_another_acceptance_or_authority() {
        let (mut board, proposal) = example();
        assert!(board.state.accept("Bob", &proposal));
        let admitted = board.admit(&proposal);
        let mut changed = proposal.clone();
        changed.to = "Carol".into();
        assert_eq!(
            board.admit(&changed).decision.kind,
            DecisionKind::DecisionRequired
        );
        assert!(matches!(
            board.execute(&changed, &admitted).outcome,
            ExecutionOutcome::Rejected { .. }
        ));
        assert_eq!(board.state.assignee, "Alice");
    }

    #[test]
    fn effective_revocation_after_admission_blocks_dispatch() {
        let (mut board, proposal) = example();
        assert!(board.state.accept("Bob", &proposal));
        let admitted = board.admit(&proposal);
        board.state.revoke_delegation();
        assert!(matches!(
            board.execute(&proposal, &admitted).outcome,
            ExecutionOutcome::Rejected { .. }
        ));
        assert_eq!(board.state.assignee, "Alice");
        assert_eq!(
            board.admit(&proposal).decision.kind,
            DecisionKind::Prevented
        );
    }

    #[test]
    fn stale_task_revision_and_missing_delegation_prevent_admission() {
        let (mut board, mut proposal) = example();
        proposal.expected_revision = 2;
        assert_eq!(
            board.admit(&proposal).decision.kind,
            DecisionKind::Prevented
        );
        board.state.revoke_delegation();
        proposal.expected_revision = 3;
        assert_eq!(
            board.admit(&proposal).decision.kind,
            DecisionKind::Prevented
        );
    }
}
