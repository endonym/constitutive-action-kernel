# Host persistence and adapter conformance for v1

Use this guide to test your application's storage, reservation and adapter behaviour. Implement the [`host_conformance` module](../src/host_conformance.rs) traits `HostHandle` and `HostScenario`, then run `check_host` against your integration. The probe uses your host's records and execution controls without changing the v1 wire format. The [reference test](../tests/host_conformance_reference.rs) shows an in-memory implementation.

## Required state and ordering

For each proposal, save the complete v1 intent, permitted decision and its source snapshot or immutable references before reserving an execution attempt. Keep a unique proposal ID mapped to one intent fingerprint. Repeating that ID with different content is a conflict, even after an uncertain or terminal result. The adapter must reconstruct the effect it is about to perform from its typed operation and compare the profile ID/version, operation, destination and canonical payload to the persisted admitted effect. `check_pre_dispatch` does not reserve an attempt. Your store must prevent two callers from creating separate attempts for the same proposal.

Use a transaction or equivalent concurrency control to establish whether reservation or revocation happened first. A lock, compare-and-swap or database constraint must give both operations a single order for the same authority. Within the reservation transaction, return any existing attempt and outcome first. For a new reservation, reconstruct admission from the saved intent and snapshot and require its decision to match the saved decision. Check current context, profile, sequence, expiry and effective revocation before committing one attempt with a unique proposal key and immutable replay ID.

A revocation ordered before reservation denies a new attempt. A revocation ordered later cannot erase an already committed attempt or its outcome. A pending request alone does not revoke authority; any temporary suspension needs its own authorised policy.

The reported values may be authority epochs rather than unique claim numbers: different claims can share an epoch, but a winning claim must report an epoch earlier than a subsequent effective revocation. These values must reflect the order enforced by the transaction or equivalent concurrency control.

The attempt begins `in_flight` when reservation commits. Store a `confirmed` occurrence only with adapter or external evidence sufficient to identify the occurrence. Store a definitive `rejected` outcome only when host or adapter evidence establishes that the attempted effect did not occur. Until reconciled, a timeout, lost response or crash leaves the original attempt in flight or uncertain.

Reopening a host must return the original attempt and external replay ID. Any retry uses that same external idempotency identity, or first queries/reconciles the original operation. If the service supports neither idempotency nor reliable reconciliation, do not start a second effect while uncertainty remains.

Expose your host's ticket, replay ID and attempt state through the probe's test types. This does not change the v1 wire schema. `ActionReceipt::attempt_id` is an internal identifier unless your host explicitly maps it to the external idempotency key.

When the effect is outside the host store’s transaction, reconcile it with the host record. After a crash, retain the original attempt and replay identity. Recovery may confirm the outcome once it has reconciled adapter evidence.

## Running the probe in another Rust host

Implement `HostScenario::open` to construct a fresh handle against the same disposable store, and have the factory passed to `check_host` create an independent namespace per scenario. For a real adapter, use `check_host_with_fixture` and return a fresh scenario, a host-native intent with a unique proposal ID, and an admitted snapshot each time. Map the probe methods to your application:

| Method | Application operation |
| --- | --- |
| `prepare` | Evaluate the v1 intent and snapshot, save the proposal, and reject a proposal ID reused for different content. |
| `reserve` | Use the real transaction path to check current authority and reserve at most one attempt. |
| `effective_revoke` | Record effective revocation using the same ordering mechanism as reservation. |
| `record_uncertain` | Save the uncertain outcome against the original attempt. |
| `adapter_matches` | Run the adapter's actual effect comparison without executing an external effect. |

For example:

```rust,ignore
#[test]
fn host_lifecycle_conforms() {
    constitutive_action::host_conformance::check_host_with_fixture(|| TestHostScenario::fresh_fixture());
}
```

The probe checks adapter comparison against altered effects and replay IDs, proposal-ID conflicts, one new reservation under concurrent claims, stable replay through reopened handles, persisted uncertainty, and sequential and racing revocation orders. Use a persistent test store and fresh handles for the reopen checks.

For example, call this helper from an integration test with a fresh persistent scenario and a permitted fixture:

```rust
use constitutive_action::host_conformance::{
    AttemptState, HostHandle, HostScenario, Reservation,
};
use constitutive_action::v1::{ActionIntent, EvaluationSnapshot};

fn uncertain_attempt_survives_reopen<S: HostScenario>(
    scenario: &S, intent: &ActionIntent, snapshot: &EvaluationSnapshot,
) {
    let mut host = scenario.open();
    host.prepare(intent, snapshot).unwrap();
    let Reservation::New(ticket) = host.reserve(&intent.proposal_id).unwrap() else {
        panic!("Expected a fresh reservation");
    };
    host.record_uncertain(&ticket.attempt_id, "Response lost").unwrap();
    drop(host);

    let mut reopened = scenario.open();
    let Reservation::Replay { ticket: replay, state } =
        reopened.reserve(&intent.proposal_id).unwrap()
    else {
        panic!("Reopening must return the original attempt");
    };
    assert_eq!(replay, ticket);
    assert!(matches!(state, AttemptState::Uncertain { .. }));
}
```

This checks persistence across fresh handles; the process-crash checks below exercise interrupted execution.

Separately inject process crashes at reservation commit, after external success before receipt persistence, and during reconciliation. Verify the external service's idempotency/query behaviour, test capability isolation, and check that a live adapter execution produces the reported result. These checks complement the in-memory reference test and receipt-structure validation.

TGC-Demo runs the probe against its durable host and local mock adapter. Its host tests run separately from the Kernel’s CI; see [integration evidence](integration-evidence.md).
