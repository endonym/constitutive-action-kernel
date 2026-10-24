# Integration evidence for v1

This document records observed behavior of the v1 action contract in the task-board demonstration and TGC-Demo's durable local mock-effect host. Each observation identifies the effect path and the evidence supporting it.

## Task-board example

The [v1 task-reassignment example](../examples/v1_task_reassignment.rs) uses an in-memory board and a host-owned adapter. It exercises exact-effect binding, a response attributed to Bob for particular terms, rejection of an altered recipient, a saved receipt on replay, and effective withdrawal before dispatch. Attributed identity and verification assertions are supplied by the demonstration host; its replay map is in memory.

## TGC-Demo: constituted authority and durable local effects

The [Activity Container demonstration](https://thegenerativecommons.org/activity-container.html) uses the kernel for participant commitments, scoped delegation, separately authorised exceptions, interpretation changes and independent recipient decisions. Its host persists the exact intent, evidence, reservation and receipt. A separately persisted mock adapter compares the admitted effect and records it under the original replay identity. Identity and standing use simulated Proof of Honesty; all financial records and approvals are synthetic.

The TGC-Demo repository supplies executable evidence on that path:

- `src/store.rs`: `persistent_host_passes_official_conformance_including_races` runs the host-conformance probe against the durable store and the mock adapter’s effect comparison, including concurrent reservation and revocation ordering.
- `tests/cli.rs`: `process_crashes_recover_original_attempt_without_double_effects` terminates child processes after reservation and after external mock success, then recovers the original attempt without a second occurrence.
- `tests/authority_lifecycle.rs`: 32 governance crash/recovery cases cover proposal, activation, amendment and promotion across both authority topologies, retaining consent and replay identity. Additional cases verify that recovery applies the activity’s migration policy.
- `tests/queue_recovery.rs`: process crashes around the pending-work commit test decision/queue atomicity and resolution recovery.
- `tests/ownership_authorship.rs`: exclusive activity ownership, recovery after owner termination, and interpretation authorship bound to the publishing credential and Rulebook.
- `tests/kernel_contract.rs`: execution authority expires with required evidence and acceptance.

The 23 October 2026 validation recorded 104 passing workspace test results and 48 passing standalone kernel test results, with no failures or ignored tests. These suites overlap and are not additive counts of distinct obligations. All twelve narrative branches and six executable examples passed. To capture current results, test stores, source inputs and SHA-256 fingerprints, run `python3 tools/verify.py --out NEW_DIRECTORY` from TGC-Demo.

## Tested effect paths

TGC-Demo establishes the local mock-record path, including durable constituted authority and crash recovery. For another resource, implement and test its authority resolution, acceptance and policy assessments, adapter comparison and recovery under the same action contract.

In TGC-Demo, `./check --no-fail-fast` runs the workspace suite, including its persistent-host regressions. The Kernel’s CI runs `cargo test --all-targets --locked`, including library, integration and example tests; each host runs its own integration tests.
