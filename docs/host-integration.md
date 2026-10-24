# Integrating the v1 action API in a Rust host

This is the integration sequence for a host that controls a consequential effect. The Kernel evaluates proposed actions; the host verifies evidence, controls credentials, reserves dispatch and executes through its controlled adapter. The [task-reassignment demonstration](../examples/v1_task_reassignment.rs) shows concrete construction of an intent, profile, evidence, acceptance and receipt using in-memory state. A deployment uses durable reservation.

## The action path

1. **Define the effect and resolve authority.** The host authenticates the actor and represented principal, decides which authority and versioned `GateProfile` apply, and creates a `v1::ActionIntent`. Its `Effect` contains a profile ID and version, operation, destination and profile-defined canonical payload bytes. Keep a proposal ID stable for retries; assign a new ID to a changed intent. The host resolves a complete authoritative `EvaluationSnapshot`: required conditions or an intentionally selected `ConditionPolicy::ExplicitOpen`, attributed assessments, evidence references, exact-term acceptances and pending/effective revocation state. A `VerificationStatus::Verified` value records the host or verifier's assessment of the underlying proof.

2. **Evaluate, then persist.** Call the real API:

   ```rust
   use constitutive_action::v1::{DecisionKind, evaluate};

   let admission = evaluate(&intent, &snapshot);
   if admission.decision.kind != DecisionKind::Permitted {
       // Host: persist decision evidence.
       // DecisionRequired may trigger an authenticated acceptance workflow.
       return;
   }
   ```

   The four results are `Permitted`, `Prevented`, `DecisionRequired` and `Indeterminate`. Only `Permitted` carries an `ExecutionAuthority`. Persist the exact intent, snapshot (or immutable, retrievable sources) and decision **before** attempting an effect; enforce a unique proposal-ID-to-fingerprint mapping. After reopening, reconstruct `Admission` by evaluating the saved intent and snapshot, and require its decision to match the saved decision. For a new reservation, also check current live state. `ActionIntent::fingerprint()` and the decision's evidence/profile/basis fingerprints identify supplied bytes and records; the host authenticates their sources. A changed offer, acceptance, profile or material authority state needs a fresh evaluation. A pending revocation request remains distinct from an effective decision.

3. **Reserve one attempt against live state.** In a host transaction, serialise the relevant authority with effective revocation and return any existing attempt and outcome first. For a new reservation, reread current context/profile/version/sequence and reject stale or withdrawn authority. For an admitted action, `admission.authority.as_ref().unwrap().check_pre_dispatch(&intent, &live_state)` checks exact binding and supplied live state. The issued authority expires at the earliest of snapshot validity, used evidence validity and required acceptance validity; unused evidence does not shorten it. Expiry prevents a new reservation, while recovery preserves an already-reserved attempt. In that same ordering boundary, the host inserts a unique attempt and an immutable external replay/idempotency ID, then commits before dispatch. An effective revocation committed first denies a new reservation; one committed later leaves an in-flight attempt intact. Re-evaluate when applicable conditions change outside the live-state checks. Temporary suspension while a revocation request is pending requires an explicit policy.

   Host pseudocode:

   ```text
   transaction:
     lock authority; load saved records
     reject conflicting proposal fingerprint
     if an attempt exists: return its ticket and outcome
     reconstruct admission; require Permitted and a decision matching the saved decision
     read current state; reject stale conditions or effective revocation
     require check_pre_dispatch(original_intent, current_live_state) to succeed
     insert one attempt with a stable replay ID; commit
   ```

4. **Execute through a controlled adapter.** The host keeps effectful credentials and tools outside the agent's reach. The adapter reconstructs the *actual* typed effect and compares its profile ID/version, operation, destination and canonical payload with the persisted admitted `Effect`. It sends under the reserved replay identity. This applies to internal state changes as well as network calls. The host invokes the adapter after the kernel decision and dispatch reservation.

5. **Record and recover the outcome.** Construct an `ActionReceipt` referring to the original proposal, fingerprint and decision, with executor and attempt IDs for a dispatched effect. `ExecutionOutcome` distinguishes `NotDispatched`, `Confirmed { occurrence_id }`, `Rejected { reason }` and `Uncertain { reason }`. `validate_receipt(&intent, &admission, &receipt)` checks their structural relationship; the host substantiates an occurrence from its adapter or external service. If a response is lost, keep the original attempt uncertain. On restart, query/reconcile the original external operation or retry with the **same external replay identity** where the service guarantees idempotency. Preserve the original attempt until its outcome is known. A later revocation leaves the outcome record of an earlier committed attempt intact. `ActionReceipt::attempt_id` is an internal record identifier unless the host explicitly maps it to the external replay ID.

The persistence and dispatch portions above are **host protocol**. Implement the [`host_conformance` probe](host-conformance.md) against a disposable instance of the real store and adapter path. Test crash gaps, external reconciliation and capability isolation in the host integration.

## Wire and version boundaries

Pin an immutable source revision now; after a tagged release, pin that release. `0.x` minor versions may break Rust or serialised compatibility. Use `v1::SCHEMA_VERSION` (`1`) in `ActionIntent`, `EvaluationSnapshot`, `LiveAuthorityState` and `ActionReceipt`; `GateDecision` also carries its schema version. Keep that wire version distinct from the crate version, `Effect::profile_version` and `GateProfile::version`. Most v1 record structs reject unknown fields during deserialisation, and `evaluate` returns `Indeterminate` for unsupported or inconsistent schema/context inputs. Records without receipt/live-state version fields deserialize as version `0` for inspection; `validate_receipt` and `check_pre_dispatch` deny them current v1 authority. Older decisions without `decision_basis_fingerprint` remain readable but cannot validate a current v1 receipt. A host must deny authority for unknown versions or required fields at every persistence and exchange boundary. The [golden v1 wire fixtures](../tests/wire_fixture.rs) fix JSON representations and the intent fingerprint; in that JSON, `canonical_payload` is an array of bytes. Profile-specific canonical encoding and terms digests belong to the host's versioned domain contract. Apply the activity’s version and migration policy before recovery can dispatch. Preserve historical records; reconstitute authority where that policy requires it.

The integrated action boundary ensures that **the adapter performs no wider effect than the one admitted under recorded authority**. Verified inputs, complete mediation, adapter comparison and occurrence evidence establish the guarantee. Test capability isolation alongside gate and receipt checks.
