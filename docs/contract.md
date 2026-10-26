# Constitutive action kernel: action contract

**Version:** `0.1.0` provides the versioned v1 action gate, available as `constitutive_action::v1`. This document defines kernel checks and the host action lifecycle. Persistence, evidence verification and effect execution belong to the integrating host.

## Purpose and claim

The Kernel evaluates exact proposed effects against host-supplied conditions, evidence and required consent. Activity Field Containers use it with either **controlled** or **constitutive** authority. An agent may propose an action, but the capability that makes a consequential effect real remains with a host-controlled adapter. The host resolves the applicable commitments, authority and evidence; the kernel admits, prevents, requests a decision or returns an indeterminate result for the *exact* proposed effect. An admitted effect can then be dispatched under narrow, current authority and joined to a record of what occurred or failed.

The Kernel binds each decision to the exact proposed action, its governing conditions, and evidence. Its common contract identifies proposals, authority, evidence, decisions and effects. An integrating environment supplies the activity-specific conditions and an adapter for the effect it controls. Substantive judgements about those conditions belong to the activity and its participants.

The system guarantee is: **for a mediated effect, the controlled adapter executes no broader effect than the one admitted under recorded authority**. The host authenticates inputs, holds the effectful capability, and preserves evidence of the adapter's actual outcome. The kernel's decision and the host's receipt connect that outcome to the exact governed proposal.

## Versioned action API

The v1 API binds an exact proposed effect, an applicable profile and a supplied evidence snapshot. It returns `Permitted`, `Prevented`, `DecisionRequired` or `Indeterminate`, with reasons; only `Permitted` carries an `ExecutionAuthority`. A host can resolve an outstanding decision or changed condition and re-evaluate before dispatch. The authority checks the exact action and current supplied state, while the receipt validator relates the decision to a reported technical outcome.

The v1 module uses a schema-versioned envelope. It binds the effect-profile identity and version, operation, destination and canonical payload bytes supplied by the host; fingerprints the gate profile, assessed conditions, evidence, acceptance responses, revocation state and snapshot validity; and requires either an explicit-open policy or named required conditions. Snapshot assessment, acceptance and revocation-request collections are sorted before the decision-basis digest is calculated, so their traversal order does not change it. Profile requirement order remains part of the profile fingerprint. The host authenticates the supplied records that these digests identify.

`ActionIntent`, `EvaluationSnapshot` and `GateDecision` require an explicit schema version. `LiveAuthorityState` and `ActionReceipt` emit `schema_version: 1`; JSON without that field deserialises as version 0 for inspection only. Version 0 and unsupported future versions cannot authorise dispatch or validate a new v1 receipt. A v1 decision written before the decision-basis field was added can likewise be inspected but cannot validate a new receipt. The module rejects unknown wire fields; unsupported schema versions cannot produce current dispatch authority or valid current receipts.

The host selects the profile, authenticates identities, verifies the sources behind assertions and supplies a complete snapshot. It durably reserves dispatch, substantiates external occurrences and routes revocation decisions through its own protocol. The [task-reassignment example](../examples/v1_task_reassignment.rs) exercises the API with an in-memory host; [integration evidence](integration-evidence.md) records tests of TGC-Demo’s persistent host and separately persisted local mock adapter.

## Host lifecycle and versioned action records

Each effect profile defines which operation parameters must be bound to the decision and how to encode them consistently. The intent’s fingerprint binds its identifying fields and canonical payload bytes. The adapter must compare the actual effect against that binding. Proposal IDs may not be reused with different bindings.

For example, a task-reassignment profile can encode its parameters with length-prefixed strings and a fixed-width revision:

```rust
use constitutive_action::v1::Effect;

fn reassignment_effect(task: &str, from: &str, to: &str, revision: u64) -> Effect {
    let mut bytes = b"project.task.reassign/v1".to_vec();
    for field in [task, from, to] {
        bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    bytes.extend_from_slice(&revision.to_be_bytes());
    Effect {
        profile_id: "project.task.reassign".into(),
        profile_version: 1,
        operation: "reassign".into(),
        destination: format!("task:{task}"),
        canonical_payload: bytes,
    }
}
```

The host and adapter use the same encoding. Changing the task, either assignee or the expected revision changes the effect being authorised.

The host builds the snapshot from authoritative sources and sets the sequence interval for which it is valid. The snapshot contains the applicable profile, assessments, evidence, acceptances and revocation state. Each assessment identifies its condition and version, assessor, result, reason and evidence references. Evidence records identify their type and version, subject, issuer, content digest, verification status, verifier and validity interval. Required evidence must be verified and current. Domain profiles add the fields their activity requires.

The kernel checks structural bindings and fails closed when required evidence is absent, unverified, stale or internally inconsistent. The host or an explicitly trusted verifier establishes identities, standing, delegation, semantic assessments and policy meaning. The host evaluates model-generated statements under an identified assessor and policy before recording condition results. During condition evaluation, unresolved or invalid required inputs produce `Indeterminate`; a complete assessment with a failed condition produces `Prevented`. Once conditions pass, a valid refusal produces `Prevented`; an outstanding acceptance—including an expired or mismatched response—produces `DecisionRequired`. Effective revocation also prevents execution. Only a complete, current and passing snapshot can permit. Required unknown or structurally inconsistent inputs cannot produce permission. The host resolves conflicting assessments or marks the condition unknown.

Starting with the task-board example’s otherwise valid snapshot, mark a required assessment unresolved when its source is unavailable:

```rust
use constitutive_action::v1::{CheckResult, DecisionKind, evaluate};

let mut unresolved = snapshot.clone();
unresolved.assessments[0].result = CheckResult::Unknown;
unresolved.assessments[0].reason = "Required source is unavailable".into();
unresolved.assessments[0].evidence_ids.clear();

let admission = evaluate(&intent, &unresolved);
assert_eq!(admission.decision.kind, DecisionKind::Indeterminate);
assert!(admission.authority.is_none());
```

### Identity and standing

The host authenticates identity and standing through the systems appropriate to the activity, including institutional systems or Proof of Honesty. Where standing is required, the host supplies attributed, verified evidence for the applicable condition. The Kernel checks its structural bindings and validity; the host retains the supporting records.

### Acceptance

Preparing or proposing an action is not authority to execute it. The host supplies the originating delegation’s constraints as required conditions; acceptance does not override them. A request for acceptance identifies the authorised decision-maker and the exact consequential terms being offered. An acceptance or refusal is an attributed response bound to the action fingerprint and the digest of the consequential terms. Silence remains pending. A changed effect, recipient, obligation or applicable terms invalidates the prior acceptance and requires re-evaluation; a response to an old proposal cannot be replayed for a new one. The host determines who may accept and how that decision is authenticated. The kernel validates supplied bindings and returns a decision; the host records the resulting state.

For the task-board example’s single required acceptance, the host first authenticates the responder and checks the exact offer they accepted. The response values below come from that authenticated approval. With the other conditions passing, attach the response to the mutable snapshot and re-evaluate:

```rust
use constitutive_action::v1::{AcceptanceRecord, AcceptanceState, DecisionKind, evaluate};

let requirement = &snapshot.profile.required_acceptances[0];
let response = AcceptanceRecord {
    requirement_id: requirement.requirement_id.clone(),
    state: AcceptanceState::Accepted {
        by_principal_id: authenticated_responder.to_owned(),
        action_fingerprint: intent.fingerprint(),
        terms_digest: accepted_terms_digest.to_owned(),
        at_sequence: accepted_at,
        valid_through_sequence: approval_valid_through,
    },
};
snapshot.acceptances = vec![response];
let admission = evaluate(&intent, &snapshot);
assert_eq!(admission.decision.kind, DecisionKind::Permitted);
```

### Revocation and suspension

A **revocation request** is an attributed proposal concerning a particular delegation or authority; it is distinct from effective revocation. The v1 record holds a requester, target, routing destination and sequence. The host routes it, records delivery and decision status, and establishes the effective version or sequence at which future authority is withdrawn. A deployment may separately define temporary suspension while a request is pending; that suspension is an explicit policy decision.

After the host commits an effective revocation, include that decision in the snapshot. Here `withdrawal:7` represents the committed record, effective at the snapshot’s sequence:

```rust
use constitutive_action::v1::{DecisionKind, EffectiveRevocation, evaluate};

let mut revoked = snapshot.clone();
revoked.revocation.effective = Some(EffectiveRevocation {
    decision_id: "withdrawal:7".into(),
    target_authority_id: intent.authority_id.clone(),
    effective_at_sequence: snapshot.sequence,
});
assert_eq!(evaluate(&intent, &revoked).decision.kind, DecisionKind::Prevented);
```

The host must serialise an effective authority change against dispatch reservation. A revocation effective before reservation prevents that dispatch. Once an exact attempt has been committed to dispatch, an external effect may already be in flight; a later revocation cannot cancel it retroactively. The host’s action record preserves reservation and revocation ordering alongside the receipt and outcome. V1 distinguishes a supplied pending request from a supplied effective decision; the host establishes the decision process and transactional ordering boundary.

### Dispatch, retry and occurrence

Execution authority binds the exact effect and expires no later than its snapshot, used evidence or required acceptances. The host supplies the current sequence when reserving dispatch. The host durably records the proposal and decision before dispatch, then atomically reserves one permitted attempt against current authority, versions, expiry and effective revocation. `check_pre_dispatch` checks authority but does not reserve or consume an attempt; the host must enforce one reservation per proposal. A retry of an uncertain external operation must reuse the original idempotency identity or reconcile the original operation; it must not silently become a second new effect. Where an external system cannot provide idempotency or queryable outcomes, uncertainty remains explicit and the host must not invent confirmation.

The contract distinguishes admission, dispatch reservation, in-flight attempt, confirmed occurrence, definitive rejection and uncertain outcome across the host store and external service. The adapter reports the external result; the host associates it with the original exact intent and attempt. Internal state changes are consequential effects too and use the same boundary, even without a network call.

### Decision evidence and receipts

A gate decision records its kind, action binding, context and sequence, profile identity/version/fingerprint, evidence IDs and basis fingerprints, and reasons. The host retains the underlying snapshot to recover the applicable conditions and their assessments. A refused, expired or abandoned proposal can have decision evidence without an occurrence. A composite host action record additionally identifies the adapter/executor, idempotency identity, dispatch attempt, technical outcome and an internal or external occurrence ID only when confirmed. `ActionReceipt` records the execution result. Any interpretation of that result or assertion that it satisfied an obligation belongs in a separate record identifying who made the assessment.

Receipts should be verifiable against versioned, content-addressed evidence without unnecessarily publishing confidential payloads. What an audience may inspect and how long evidence is retained are host policies. A digest or signature supports integrity and attribution under a specified trust model; neither proves that a verifier was trustworthy or that an adapter had no bypass path.

## Host and adapter obligations

An integrating host must:

1. Identify the acting agent, represented principal, authorised decision-makers and the source of their authority; resolve applicable domain conditions and evidence at an authoritative version.
2. Verify or obtain attributed verification of standing and other external evidence; distinguish mechanically checked conditions from fallible semantic assessments and unknowns.
3. Keep protected reads, credentials and effectful tools unavailable to the agent except through controlled paths. The adapter must compare the actual effect with the admitted binding and reject any widening or alteration.
4. Persist proposal, decision, acceptance, revocation and dispatch transitions with ordering and replay protection. Use a transaction or equivalent concurrency control to establish whether revocation or reservation took effect first.
5. Use an idempotent or reconcilable execution protocol for each supported effect, preserve uncertainty honestly and record confirmed occurrences only on sufficient evidence.
6. Govern disclosure of private proposals, evidence, receipts and observations. Where the activity requires it, check authority before disclosing protected information to an agent.

The host fulfils these obligations. Adapter isolation, integration tests and operational evidence demonstrate the deployed boundary.

## Versioning and compatibility policy

The crate is at `0.1.0`. During the `0.x` series, minor releases may change Rust source or serialised compatibility; patch releases preserve it. This compatibility policy is separate from the kernel's four decision meanings and from the evolving activities built on it. `constitutive_action::v1` is the action API. Fingerprint domain strings remain stable to preserve recorded bindings. Each persisted or exchanged contract envelope identifies its schema version independently of the crate version. Readers deny authority for unsupported required fields or versions. Golden encoding/fingerprint fixtures and compatibility tests let a host pin, migrate and verify published records.

Consumers pin the current source revision, and later an immutable tagged release, while mapping their domain records into the common contract. The task-board example and TGC-Demo exercise different effect profiles; further integrations supply their own controlled adapters and authoritative state. Source distribution and package metadata use the [Generative Commons License (GCL–0.x)](../LICENSE).

## Required conformance scenarios

For each integration, run conformance scenarios covering an altered operation, destination, recipient, payload or context against a prior admission; a reused proposal ID with different content; missing, forged, expired and unverified standing; unknown or omitted required evidence; acceptance pending, refused, expired or invalidated by changed terms; a revocation request pending without an effective decision; effective revocation before dispatch; an in-flight dispatch followed by revocation; replay and restart after an uncertain external outcome; and a confirmed receipt reconciled to adapter evidence. Run these scenarios against distinct effect profiles and their actual host paths, including the adapter that mediates each effect.
