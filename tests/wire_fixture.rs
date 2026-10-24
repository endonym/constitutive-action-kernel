//! Cross-release fixtures for the v1 wire envelopes.
//! Changing either value requires a deliberate schema/profile migration.

use constitutive_action::v1::{
    ActionIntent, ActionReceipt, ConditionPolicy, DecisionKind, Effect, EvaluationSnapshot,
    ExecutionOutcome, GateDecision, GateProfile, LiveAuthorityState, ReceiptError, RevocationState,
    SCHEMA_VERSION, evaluate, validate_receipt,
};

const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn v1_import_path_preserves_the_original_module() {
    assert_eq!(
        std::any::TypeId::of::<ActionIntent>(),
        std::any::TypeId::of::<constitutive_action::experimental_v1::ActionIntent>()
    );
}

fn golden_intent() -> ActionIntent {
    ActionIntent {
        schema_version: SCHEMA_VERSION,
        proposal_id: "proposal:golden-1".into(),
        actor_id: "agent:alice".into(),
        represented_principal_id: "alice".into(),
        authority_id: "delegation:1".into(),
        context_id: "activity:alpha".into(),
        context_version: 7,
        effect: Effect {
            profile_id: "task.reassign".into(),
            profile_version: 1,
            operation: "reassign".into(),
            destination: "task:42".into(),
            canonical_payload: b"from=alice;to=bob".to_vec(),
        },
    }
}

fn golden_decision() -> GateDecision {
    GateDecision {
        schema_version: SCHEMA_VERSION,
        kind: DecisionKind::Prevented,
        reasons: vec!["deliberate fixture".into()],
        action_fingerprint: DIGEST.into(),
        context_id: "activity:alpha".into(),
        context_version: 7,
        sequence: 9,
        profile_id: "task.gate".into(),
        profile_version: 1,
        profile_fingerprint: DIGEST.into(),
        evidence_ids: vec![],
        evidence_snapshot_fingerprint: DIGEST.into(),
        decision_basis_fingerprint: DIGEST.into(),
    }
}

fn golden_decision_wire() -> String {
    format!(
        r#"{{"schema_version":1,"kind":"prevented","reasons":["deliberate fixture"],"action_fingerprint":"{DIGEST}","context_id":"activity:alpha","context_version":7,"sequence":9,"profile_id":"task.gate","profile_version":1,"profile_fingerprint":"{DIGEST}","evidence_ids":[],"evidence_snapshot_fingerprint":"{DIGEST}","decision_basis_fingerprint":"{DIGEST}"}}"#
    )
}

#[test]
fn v1_intent_wire_and_fingerprint_are_stable() {
    let intent = golden_intent();
    let wire = r#"{"schema_version":1,"proposal_id":"proposal:golden-1","actor_id":"agent:alice","represented_principal_id":"alice","authority_id":"delegation:1","context_id":"activity:alpha","context_version":7,"effect":{"profile_id":"task.reassign","profile_version":1,"operation":"reassign","destination":"task:42","canonical_payload":[102,114,111,109,61,97,108,105,99,101,59,116,111,61,98,111,98]}}"#;
    assert_eq!(serde_json::to_string(&intent).unwrap(), wire);
    assert_eq!(serde_json::from_str::<ActionIntent>(wire).unwrap(), intent);
    assert_eq!(
        intent.fingerprint(),
        "51892c620bdff1ad03bc28854e1b463a4bf5d8b2f335e90cc476b2217492e93e"
    );
}

#[test]
fn v1_intent_rejects_unrecognised_wire_fields() {
    let mut value = serde_json::to_value(golden_intent()).unwrap();
    value["unrecognised_authority"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ActionIntent>(value).is_err());
}

#[test]
fn v1_live_authority_wire_is_versioned_and_legacy_is_read_only() {
    let live = LiveAuthorityState {
        schema_version: SCHEMA_VERSION,
        authority_id: "delegation:1".into(),
        context_id: "activity:alpha".into(),
        context_version: 7,
        profile_fingerprint: DIGEST.into(),
        sequence: 9,
        revocation: RevocationState::default(),
    };
    let wire = format!(
        r#"{{"schema_version":1,"authority_id":"delegation:1","context_id":"activity:alpha","context_version":7,"profile_fingerprint":"{DIGEST}","sequence":9,"revocation":{{"pending_requests":[],"effective":null}}}}"#
    );
    assert_eq!(serde_json::to_string(&live).unwrap(), wire);
    assert_eq!(
        serde_json::from_str::<LiveAuthorityState>(&wire).unwrap(),
        live
    );

    let mut old = serde_json::to_value(&live).unwrap();
    old.as_object_mut().unwrap().remove("schema_version");
    let legacy: LiveAuthorityState = serde_json::from_value(old).unwrap();
    assert_eq!(legacy.schema_version, 0);
    let intent = golden_intent();
    let snapshot = EvaluationSnapshot {
        schema_version: SCHEMA_VERSION,
        context_id: intent.context_id.clone(),
        context_version: intent.context_version,
        sequence: live.sequence,
        valid_through_sequence: live.sequence,
        profile: GateProfile {
            profile_id: "task.gate".into(),
            version: 1,
            effect_profile_id: intent.effect.profile_id.clone(),
            effect_profile_version: intent.effect.profile_version,
            conditions: ConditionPolicy::ExplicitOpen,
            required_acceptances: vec![],
        },
        evidence: vec![],
        assessments: vec![],
        acceptances: vec![],
        revocation: RevocationState::default(),
    };
    let admission = evaluate(&intent, &snapshot);
    assert_eq!(admission.decision.kind, DecisionKind::Permitted);
    let mut current = live;
    current.profile_fingerprint = snapshot.profile.fingerprint();
    admission
        .authority
        .as_ref()
        .unwrap()
        .check_pre_dispatch(&intent, &current)
        .unwrap();
    current.schema_version = 0;
    assert!(
        admission
            .authority
            .as_ref()
            .unwrap()
            .check_pre_dispatch(&intent, &current)
            .is_err()
    );
    current.schema_version = 2;
    assert!(
        admission
            .authority
            .as_ref()
            .unwrap()
            .check_pre_dispatch(&intent, &current)
            .is_err()
    );
}

#[test]
fn v1_receipt_wire_is_versioned_and_legacy_cannot_validate() {
    let receipt = ActionReceipt {
        schema_version: SCHEMA_VERSION,
        proposal_id: "proposal:golden-1".into(),
        action_fingerprint: DIGEST.into(),
        decision: golden_decision(),
        executor_id: None,
        attempt_id: None,
        outcome: ExecutionOutcome::NotDispatched,
    };
    let decision_wire = golden_decision_wire();
    let wire = format!(
        r#"{{"schema_version":1,"proposal_id":"proposal:golden-1","action_fingerprint":"{DIGEST}","decision":{decision_wire},"executor_id":null,"attempt_id":null,"outcome":"not_dispatched"}}"#
    );
    assert_eq!(serde_json::to_string(&receipt).unwrap(), wire);
    assert_eq!(
        serde_json::from_str::<ActionReceipt>(&wire).unwrap(),
        receipt
    );

    let mut old = serde_json::to_value(&receipt).unwrap();
    old.as_object_mut().unwrap().remove("schema_version");
    let legacy: ActionReceipt = serde_json::from_value(old).unwrap();
    assert_eq!(legacy.schema_version, 0);

    let intent = golden_intent();
    let snapshot = EvaluationSnapshot {
        schema_version: SCHEMA_VERSION,
        context_id: intent.context_id.clone(),
        context_version: intent.context_version,
        sequence: 9,
        valid_through_sequence: 9,
        profile: GateProfile {
            profile_id: "task.gate".into(),
            version: 1,
            effect_profile_id: intent.effect.profile_id.clone(),
            effect_profile_version: intent.effect.profile_version,
            conditions: ConditionPolicy::ExplicitOpen,
            required_acceptances: vec![],
        },
        evidence: vec![],
        assessments: vec![],
        acceptances: vec![],
        revocation: RevocationState::default(),
    };
    let admission = evaluate(&intent, &snapshot);
    let current = ActionReceipt {
        schema_version: SCHEMA_VERSION,
        proposal_id: intent.proposal_id.clone(),
        action_fingerprint: intent.fingerprint(),
        decision: admission.decision.clone(),
        executor_id: None,
        attempt_id: None,
        outcome: ExecutionOutcome::NotDispatched,
    };
    assert_eq!(validate_receipt(&intent, &admission, &current), Ok(()));
    let mut old = serde_json::to_value(&current).unwrap();
    old.as_object_mut().unwrap().remove("schema_version");
    let legacy: ActionReceipt = serde_json::from_value(old).unwrap();
    assert_eq!(
        validate_receipt(&intent, &admission, &legacy),
        Err(ReceiptError::UnsupportedVersion)
    );
    let mut future = current;
    future.schema_version = 2;
    assert_eq!(
        validate_receipt(&intent, &admission, &future),
        Err(ReceiptError::UnsupportedVersion)
    );
    let mut future_intent = intent;
    future_intent.schema_version = 2;
    assert_eq!(
        validate_receipt(&future_intent, &admission, &receipt),
        Err(ReceiptError::UnsupportedVersion)
    );
}

#[test]
fn v1_snapshot_and_decision_require_explicit_versions() {
    let intent = golden_intent();
    let snapshot = EvaluationSnapshot {
        schema_version: SCHEMA_VERSION,
        context_id: intent.context_id.clone(),
        context_version: intent.context_version,
        sequence: 9,
        valid_through_sequence: 9,
        profile: GateProfile {
            profile_id: "task.gate".into(),
            version: 1,
            effect_profile_id: intent.effect.profile_id.clone(),
            effect_profile_version: intent.effect.profile_version,
            conditions: ConditionPolicy::ExplicitOpen,
            required_acceptances: vec![],
        },
        evidence: vec![],
        assessments: vec![],
        acceptances: vec![],
        revocation: RevocationState::default(),
    };
    let admission = evaluate(&intent, &snapshot);
    let snapshot_wire = r#"{"schema_version":1,"context_id":"activity:alpha","context_version":7,"sequence":9,"valid_through_sequence":9,"profile":{"profile_id":"task.gate","version":1,"effect_profile_id":"task.reassign","effect_profile_version":1,"conditions":"explicit_open","required_acceptances":[]},"evidence":[],"assessments":[],"acceptances":[],"revocation":{"pending_requests":[],"effective":null}}"#;
    assert_eq!(serde_json::to_string(&snapshot).unwrap(), snapshot_wire);
    assert_eq!(
        serde_json::from_str::<EvaluationSnapshot>(snapshot_wire).unwrap(),
        snapshot
    );
    let decision_wire = golden_decision_wire();
    assert_eq!(
        serde_json::to_string(&golden_decision()).unwrap(),
        decision_wire
    );
    assert_eq!(
        serde_json::from_str::<GateDecision>(&decision_wire).unwrap(),
        golden_decision()
    );
    assert_eq!(admission.decision.schema_version, SCHEMA_VERSION);
    let mut no_snapshot_version = serde_json::to_value(&snapshot).unwrap();
    no_snapshot_version
        .as_object_mut()
        .unwrap()
        .remove("schema_version");
    assert!(serde_json::from_value::<EvaluationSnapshot>(no_snapshot_version).is_err());
    let mut no_decision_version = serde_json::to_value(&admission.decision).unwrap();
    no_decision_version
        .as_object_mut()
        .unwrap()
        .remove("schema_version");
    assert!(serde_json::from_value::<GateDecision>(no_decision_version).is_err());
    let mut unsupported_snapshot = snapshot;
    unsupported_snapshot.schema_version = 2;
    assert_eq!(
        evaluate(&intent, &unsupported_snapshot).decision.kind,
        DecisionKind::Indeterminate
    );
}
