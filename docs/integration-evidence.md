# Integration evidence for v1

This document links implemented behaviour to the examples and tests that exercise it. It covers the task-board example and TGC-Demo's persistent host with a local mock adapter.

## Task-board example

The [v1 task-reassignment example](../examples/v1_task_reassignment.rs) uses an in-memory board and a host-owned adapter. It exercises exact-effect binding, a response attributed to Bob for particular terms, rejection of an altered recipient, a saved receipt on replay, and effective withdrawal before dispatch. Attributed identity and verification assertions are supplied by the demonstration host; its replay map is in memory.

## TGC-Demo: authority, storage and recovery

The [Activity Container demonstration](https://thegenerativecommons.org/activity-container.html) uses the kernel for participant commitments, scoped delegation, separately authorised exceptions, interpretation changes and independent recipient decisions. Its host persists the exact intent, evidence, reservation and receipt. A separately persisted mock adapter compares the admitted effect and records it under the original replay identity. Identity and standing use simulated Proof of Honesty; all financial records and approvals are synthetic.

The TGC-Demo repository supplies executable evidence on that path:

- **Concurrent reservation and revocation:** `src/store.rs` runs `persistent_host_passes_official_conformance_including_races` against the durable store and the mock adapter's effect comparison.
- **Crash recovery without duplicate effects:** `tests/cli.rs` runs `process_crashes_recover_original_attempt_without_double_effects`, terminating child processes after reservation and after mock execution, then recovering the original attempt.
- **Consent and replay identity across governance changes:** `tests/authority_lifecycle.rs` covers 32 crash/recovery cases for proposal, activation, amendment and promotion across both authority topologies. Additional cases verify that recovery applies the activity’s migration policy.
- **Atomic decision and queue updates:** `tests/queue_recovery.rs` crashes processes around the pending-work commit and tests resolution recovery.
- **Store ownership and interpretation authorship:** `tests/ownership_authorship.rs` checks exclusive activity ownership, recovery after owner termination, and authorship bound to the publishing credential and Rulebook.
- **Authority expiry:** `tests/kernel_contract.rs` checks that execution authority expires with required evidence and acceptance.

The 23 October 2026 validation recorded 104 passing workspace test results and 48 passing standalone kernel test results, with no failures or ignored tests. These suites overlap and are not additive counts of distinct obligations. All twelve narrative branches and six executable examples passed. To capture current results, test stores, source inputs and SHA-256 fingerprints, run `python3 tools/verify.py --out NEW_DIRECTORY` from TGC-Demo.

## Tested effect paths

TGC-Demo establishes the local mock-record path, including durable constituted authority and crash recovery. For another resource, implement and test its authority resolution, acceptance and policy assessments, adapter comparison and recovery under the same action contract.

In TGC-Demo, `./check --no-fail-fast` runs the workspace suite, including its persistent-host regressions. The Kernel’s CI runs `cargo test --all-targets --locked`, including library, integration and example tests; each host runs its own integration tests.
