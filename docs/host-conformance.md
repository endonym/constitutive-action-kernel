# Host persistence and adapter conformance for v1

This guide defines the host lifecycle obligations and a reusable Rust probe. A host establishes these properties on an effect path whose capability is held by a controlled adapter and whose persistence path is tested. The [`host_conformance` module](../src/host_conformance.rs) exposes `HostHandle`, `HostScenario` and `check_host`; a host implements the traits in its integration tests without changing the v1 wire envelope. The repository's [reference test](../tests/host_conformance_reference.rs) demonstrates the probe with an in-memory implementation.

## Required state and ordering

For each proposal, persist the complete v1 intent, admissible decision and its source snapshot or immutable references before claiming dispatch. Keep a unique proposal ID mapped to one intent fingerprint. Repeating that ID with different content is a conflict, even after an uncertain or terminal result. The adapter must reconstruct the effect it is about to perform from its typed operation and compare the profile ID/version, operation, destination and canonical payload to the persisted admitted effect. A `check_pre_dispatch` success is only a precondition, not a consumed permission.

The host owns a transactional authority transition clock. An effective revocation and a dispatch reservation for that authority must take one serial order under a lock, compare-and-swap or equivalent database constraint. Within the reservation transaction, return any existing attempt and outcome first. For a new reservation, reconstruct admission from the saved intent and snapshot and require its decision to match the saved decision. Check current context, profile, sequence, expiry and effective revocation before committing one attempt with a unique proposal key and immutable replay ID. A revocation ordered before reservation denies a new attempt. A revocation ordered later cannot erase an already committed attempt or its outcome. A pending request alone does not revoke authority; any temporary suspension needs its own authorised policy. The reported values may be authority epochs rather than unique claim numbers: different claims can share an epoch, but a winning claim must report an epoch earlier than a subsequent effective revocation. The values must reflect the same serialization boundary.

The attempt begins `in_flight` when reservation commits. Store a `confirmed` occurrence only with adapter or external evidence sufficient to identify the occurrence. Store a definitive `rejected` outcome only when host or adapter evidence establishes that the attempted effect did not occur. Until reconciled, a timeout, lost response or crash leaves the original attempt in flight or uncertain. Reopening a host must return the original attempt and external replay ID. Any retry uses that same external idempotency identity, or first queries/reconciles the original operation. If the service supports neither idempotency nor reliable reconciliation, do not start a second effect while uncertainty remains. The probe's ticket, replay ID and attempt state are test-facing projections of host records; they do not add fields to the v1 wire schema or imply that `ActionReceipt::attempt_id` is an external idempotency key.

When the effect is outside the host store’s transaction, reconcile it with the host record. After a crash, retain the original attempt and replay identity. Recovery may confirm the outcome once it has reconciled adapter evidence.

## Running the probe in another Rust host

Implement `HostScenario::open` to construct a fresh handle against the same disposable store, and have the factory passed to `check_host` create an independent namespace per scenario. For a real adapter, use `check_host_with_fixture` and return a fresh scenario, a host-native intent with a unique proposal ID, and an admitted snapshot each time. Map `prepare` to the host's v1 admission and persisted proposal; `reserve` and `effective_revoke` to its actual transaction path; `record_uncertain` to its persisted attempt outcome; and `adapter_matches` to the comparison used by its capability-holding adapter in a dry-run mode. For example:

```rust,ignore
#[test]
fn host_lifecycle_conforms() {
    constitutive_action::host_conformance::check_host_with_fixture(|| TestHostScenario::fresh_fixture());
}
```

The probe checks adapter comparison against altered effects and replay IDs, proposal-ID conflicts, one new reservation under concurrent claims, stable replay through reopened handles, persisted uncertainty, and sequential and racing revocation orders. Use a persistent test store and fresh handles for the reopen checks. Separately inject process crashes at reservation commit, after external success before receipt persistence, and during reconciliation. Verify the external service's idempotency/query behaviour, test capability isolation, and observe a live adapter effect to substantiate occurrence. These checks complement the in-memory reference test and receipt-structure validation.

TGC-Demo runs the probe against its durable host and local mock adapter. Its host tests run separately from the Kernel’s CI; see [integration evidence](integration-evidence.md).
