# Constitutive action kernel

**Keep permission attached to the action.**

When an agent acts for people, authority is assessed against the exact change proposed. This Rust kernel binds a proposed effect to the applicable conditions and returns one of four reasoned admission decisions: `Permitted`, `Prevented`, `DecisionRequired` or `Indeterminate`. A host can address a failed condition or obtain a required decision and re-evaluate; a changed effect needs a fresh proposal. The host reserves dispatch, performs admitted effects through a controlled adapter, and records the outcome. An action the governing conditions prevent does not cross that effect boundary.

> For example, Alice's agent proposes assigning a task to Bob. The application's profile requires Bob to accept those exact terms. Without his acceptance the board stays unchanged. After he accepts, substituting Carol requires a fresh decision. Permission for Bob's assignment does not transfer to Carol's.

This gives a host a common action contract for:

- Binding required acceptance and assessed conditions to an exact proposal.
- Keeping missing decisions, prevention and insufficient evidence visible.
- Distinguishing permission from a confirmed, failed or uncertain execution outcome.

## Run the annotated demonstration

From the root of a local checkout, with **Rust 1.99 or later** installed:

```sh
cargo run --locked --example v1_task_reassignment
```

The example uses scripted agent proposals and an in-memory task board so you can inspect each proposal, decision and state change together. Rust 1.99 is the currently tested and declared minimum. The first run resolves the pinned dependencies. Add `--offline` after they have been cached.

The program prints an explanation, the board before and after each step, and any adapter receipt. These are its six checkpoints:

```text
1. Before Bob accepts: DecisionRequired
2. Acceptance attributed to Bob: Permitted
3. The proposal substitutes Carol: DecisionRequired
4. The adapter applies Bob's assignment: Permitted
5. A retry returns the same occurrence: Permitted
6. Effective withdrawal prevents dispatch: Prevented
```

Follow the board state as well as the gate decision:

| Step | What you observe | Why it matters |
| --- | --- | --- |
| Before acceptance | Alice remains assigned at revision 3; receipt is `NotDispatched`. | A missing required decision does not become permission. |
| Attributed acceptance | The decision becomes `Permitted`; the board is still at revision 3. | Permission and execution are separate steps. |
| Changed recipient | A fresh decision for Carol is `DecisionRequired`; dispatch using Bob's earlier admission is rejected. | A changed proposal needs its own acceptance. |
| Actual execution | The adapter assigns Bob at revision 4 and reports `board:project:greenhouse:task-7:revision:4`. | Only the adapter makes the admitted change real. |
| Retry | The saved receipt, its original `Permitted` decision and the same occurrence return; revision remains 4. No fresh permission is issued. | The demonstration's in-memory replay map prevents a second mutation. |
| Effective withdrawal | A **separate run** begins again at revision 3. Revocation takes effect before dispatch: the gate is `Prevented`, the old admission is rejected, and no change occurs. | Withdrawal can prevent future dispatch; it does not undo the first run's confirmed occurrence. |

Read the [example source](examples/v1_task_reassignment.rs) alongside the output. It constructs the effect from typed task fields, creates host-owned assessments, records a response attributed to Bob for the exact terms, and lets the board adapter perform the change. Try changing `changed.to` to another recipient and rerun: the earlier response cannot authorise the changed assignment.

For a machine-readable transcript of the **same kernel and adapter calls**:

```sh
cargo run --locked --example v1_task_reassignment -- --demo-json
```

The JSON includes decisions, action bindings, before/after board states and receipts. It is produced by the same Rust example, so a walkthrough can display the actual calls and results.

### Demonstration setup

The demonstration uses attributed identity and evidence assertions with an in-memory replay map, making the decision path visible. A deployed host authenticates those inputs and provides durable reservation, recovery and capability isolation. The [host integration guide](docs/host-integration.md) specifies the operating protocol.

Import the API as `constitutive_action::v1`. After running the example, follow the [host integration guide](docs/host-integration.md), then [test your integration](docs/host-conformance.md). Use the [contract](docs/contract.md) and [threat model](docs/threat-model.md) as references.

## Architecture

Your application defines the rules an action must satisfy and the exact operation being proposed. The Kernel evaluates those inputs; your adapter performs the permitted operation. The adapter holds the credentials or access needed to change the resource. Different applications can use the same action boundary while choosing their own rules, evidence sources and resources.

The action boundary does not require Proof of Honesty or Rulebooks. A host can use existing institutional identity, standing and authority records; where standing credentials are required, these may include Proof of Honesty. Standing does not by itself authorise an action. The kernel binds the resulting decision to the exact proposal; the host authenticates and assesses the inputs, controls dispatch, and retains evidence of the outcome. The [contract](docs/contract.md) and [threat model](docs/threat-model.md) define this division of responsibility.

## Implemented API

At `0.1.0`, the [v1 API](src/v1.rs) records the exact proposed operation, the conditions to check, the evidence used and the required consent. It distinguishes a request to revoke authority from a revocation that has taken effect, and checks that decisions and receipts refer to the same action. A profile must list required conditions or explicitly allow an open policy. Your application authenticates and verifies inputs, durably reserves each execution attempt and checks the outcome against the adapter or external service.

The [host integration guide](docs/host-integration.md) maps the API to persistence, adapter and recovery responsibilities. The [task-reassignment example](examples/v1_task_reassignment.rs) illustrates the API with an in-memory host.

Rust hosts run the [host conformance probe](docs/host-conformance.md) against their own persistence and adapter path. Alongside the in-memory reference, [integration evidence](docs/integration-evidence.md) covers TGC-Demo's durable local mock-effect host, including process-crash recovery.

## Verify the local checkout

```sh
cargo test --locked --all-targets
```

This runs the kernel tests, wire fixtures, host-conformance tests and the task-reassignment example's tests. `cargo test --locked --all-targets --offline` works once the pinned dependencies are cached. The repository tracks `Cargo.lock` to reproduce its examples and tests; consumers resolve this library through their own lockfiles.

## Distribution

The source is available from this checkout. TGC-Demo vendors a fingerprinted source snapshot. The `tgc-demo-kernel-v1` tag identifies the original demo baseline, not a versioned package release. The current evaluated demo includes later reviewed fixes not covered by that tag; the demo's source fingerprints identify its vendored working tree. Publishing the reviewed revision to GitHub remains TODO. See the [demo's evaluation evidence](https://thegenerativecommons.org/evaluate.html). No versioned package release has been published to a registry.

The source is licensed under the [Generative Commons License (GCL–0.x)](LICENSE). [∀ — Kernel Manifest](MANIFEST.md) describes this repository's control boundary; an operating host needs its own deployment-specific disclosure. `publish = false` disables publication to a Rust package registry; it does not restrict source distribution under the licence.
