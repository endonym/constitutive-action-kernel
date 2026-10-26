# Integrating the v1 action API in a Rust host

Use this guide to add the Kernel to an application that controls a resource. Your application is the host: it verifies evidence, keeps the credentials, reserves an execution attempt and calls its controlled adapter. The [task-reassignment demonstration](../examples/v1_task_reassignment.rs) shows how to build the intent, profile, evidence, acceptance and receipt using in-memory state. A deployment stores its reservations durably.

For a concrete end-to-end case, follow [permission, execution and a lost response](worked-example.md): a task assignment that succeeds at the service, loses its response, and is recovered after authority is withdrawn.

## The action path

1. **Define the effect and resolve authority.** Authenticate the actor and the principal they represent. Determine the authority and versioned `GateProfile` that apply, then create a `v1::ActionIntent`. Its `Effect` contains a profile ID and version, operation, destination and profile-defined canonical payload bytes. Keep a proposal ID stable for retries; assign a new ID to a changed intent. Build a complete `EvaluationSnapshot` from authoritative sources: required conditions or an intentionally selected `ConditionPolicy::ExplicitOpen`, attributed assessments, evidence references, exact-term acceptances and pending/effective revocation state. A `VerificationStatus::Verified` value records the host or verifier's assessment of the underlying proof.

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

   The four results are `Permitted`, `Prevented`, `DecisionRequired` and `Indeterminate`. Only `Permitted` carries an `ExecutionAuthority`. Persist the exact intent, snapshot (or immutable, retrievable sources) and decision **before** executing the action. Never associate the same proposal ID with a different intent. After reopening, reconstruct `Admission` by evaluating the saved intent and snapshot, and require its decision to match the saved decision. For a new reservation, also check current live state.

   A restore helper can compare the reconstructed decision with the saved one:

   ```rust
   use constitutive_action::v1::{
       ActionIntent, Admission, DecisionKind, EvaluationSnapshot, GateDecision, evaluate,
   };

   fn restore_admission(
       intent: &ActionIntent,
       snapshot: &EvaluationSnapshot,
       saved: &GateDecision,
   ) -> Result<Admission, &'static str> {
       let admission = evaluate(intent, snapshot);
       if &admission.decision != saved || admission.decision.kind != DecisionKind::Permitted {
           return Err("Saved admission does not match a permitted decision");
       }
       Ok(admission)
   }
   ```

   Use the saved intent and snapshot here. Before reserving a new attempt, check current authority separately.

   `ActionIntent::fingerprint()` and the decision's evidence/profile/basis fingerprints identify supplied bytes and records; the host authenticates their sources. A changed offer, acceptance, profile or material authority state needs a fresh evaluation. A pending revocation request remains distinct from an effective decision.

3. **Reserve one attempt against live state.** Use a transaction or equivalent concurrency control to order reservation against effective revocation. Return any existing attempt and outcome first. For a new reservation, reread current context/profile/version/sequence and reject stale or withdrawn authority. After requiring `Permitted`, read `live_state` from the authoritative store inside the reservation transaction and check it:

   ```rust
   let authority = admission.authority.as_ref().expect("Permitted admission");
   authority.check_pre_dispatch(&intent, &live_state)?;
   // Only after success: commit the host's unique attempt and replay key.
   ```

   Propagate any error to abort the reservation. This call checks that the action is unchanged and the supplied live state is valid.

   The issued authority expires at the earliest of snapshot validity, used evidence validity and required acceptance validity; unused evidence does not shorten it. Expiry prevents a new reservation, while recovery preserves an already-reserved attempt. Within the same transaction or equivalent ordering mechanism, insert a unique attempt and an immutable external replay/idempotency ID, then commit before dispatch. An effective revocation committed first denies a new reservation; one committed later leaves an in-flight attempt intact. Re-evaluate when applicable conditions change outside the live-state checks. Temporary suspension while a revocation request is pending requires an explicit policy.

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

4. **Execute through a controlled adapter.** Keep the credentials and tools that execute the action outside the agent's reach. The adapter reconstructs the *actual* typed effect and compares its profile ID/version, operation, destination and canonical payload with the persisted admitted `Effect`. It executes using the reserved replay identity. This applies to internal state changes as well as network calls. Call the adapter after the Kernel decision and dispatch reservation.

5. **Record and recover the outcome.** Construct an `ActionReceipt` referring to the original proposal, fingerprint and decision, with executor and attempt IDs for a dispatched effect. `ExecutionOutcome` distinguishes `NotDispatched`, `Confirmed { occurrence_id }`, `Rejected { reason }` and `Uncertain { reason }`. `validate_receipt(&intent, &admission, &receipt)` checks that these records agree. Confirm the outcome using evidence from the adapter or external service.

   For a confirmed result, use the persisted attempt ID and an occurrence ID substantiated by the adapter evidence:

   ```rust
   use constitutive_action::v1::{
       ActionReceipt, ExecutionOutcome, SCHEMA_VERSION, validate_receipt,
   };

   let receipt = ActionReceipt {
       schema_version: SCHEMA_VERSION,
       proposal_id: intent.proposal_id.clone(),
       action_fingerprint: intent.fingerprint(),
       decision: admission.decision.clone(),
       executor_id: Some("task-board-adapter".into()),
       attempt_id: Some(saved_attempt_id.to_owned()),
       outcome: ExecutionOutcome::Confirmed {
           occurrence_id: confirmed_occurrence_id.to_owned(),
       },
   };
   validate_receipt(&intent, &admission, &receipt)?;
   // Persist this receipt together with the supporting adapter evidence.
   ```

   Here `admission` is the original permitted admission. Receipt validation checks the record bindings; the adapter evidence establishes the outcome.

   If a response is lost, keep the original attempt uncertain. On restart, query/reconcile the original external operation or retry with the **same external replay identity** where the service guarantees idempotency. Preserve the original attempt until its outcome is known. A later revocation leaves the outcome record of an earlier committed attempt intact. `ActionReceipt::attempt_id` is an internal record identifier unless the host explicitly maps it to the external replay ID.

The persistence and dispatch portions above are **host protocol**. Implement the [`host_conformance` probe](host-conformance.md) against a disposable instance of the real store and adapter path. Test crash gaps, external reconciliation and capability isolation in the host integration.

## Wire and version boundaries

Pin an immutable source revision now; after a tagged release, pin that release. `0.x` minor versions may break Rust or serialised compatibility.

| Version or record | Integration requirement |
| --- | --- |
| Wire schema | Use `v1::SCHEMA_VERSION` (`1`) in `ActionIntent`, `EvaluationSnapshot`, `LiveAuthorityState` and `ActionReceipt`. `GateDecision` also carries its schema version. |
| Crate and profiles | Keep the wire schema version separate from the crate version, `Effect::profile_version` and `GateProfile::version`. |
| Unknown fields or versions | Most v1 record structs reject unknown fields during deserialisation. `evaluate` returns `Indeterminate` for unsupported or inconsistent schema/context inputs. Deny authority for unknown versions or required fields whenever records are loaded or exchanged. |
| Older receipt/live-state records | A missing schema version deserialises as `0` for inspection. `validate_receipt` and `check_pre_dispatch` reject these records for current v1 use. |
| Older decisions | Decisions without `decision_basis_fingerprint` remain readable but cannot validate a current v1 receipt. |

The [golden v1 wire fixtures](../tests/wire_fixture.rs) fix JSON representations and the intent fingerprint; `canonical_payload` is an array of bytes in JSON. Define profile-specific canonical encoding and terms digests in the host's versioned domain contract.

Apply the activity's version and migration policy before recovery can dispatch. Preserve historical records; reconstitute authority where that policy requires it.

The integrated action boundary ensures that **the adapter performs no wider effect than the one admitted under recorded authority**. Verified inputs, complete mediation, adapter comparison and occurrence evidence establish the guarantee. Test capability isolation alongside gate and receipt checks.
