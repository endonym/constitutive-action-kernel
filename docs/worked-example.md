# Worked example: permission, execution and a lost response

Alice delegates task assignment in her project to an agent. The agent proposes moving task `task-7` from Alice to Bob at task revision `3`. Project policy requires Bob to accept the exact assignment.

The existing [Rust task-board example](../examples/v1_task_reassignment.rs) shows the proposal, consent and adapter checks in memory. This worked integration extends that scenario to a host database and a separate task service. The transactions below are host pseudocode; they show where to connect your storage and service APIs.

## 1. Define the operation and the authority separately

The host builds an `ActionIntent` for proposal `proposal:42`, actor `agent:Alice`, principal `Alice`, authority `alice-task-delegation`, and project context version `1`.

| Record | What it binds in this example |
| --- | --- |
| `Effect` | Profile `project.task.reassign` version `1`, operation `reassign`, destination `task:task-7`, and canonical bytes for the task, old assignee, new assignee and expected revision. |
| `GateProfile` | Conditions `sponsor-delegation` and `task-current`, plus required acceptance from Bob. |
| `Evidence` and `ConditionAssessment` | Host-verified delegation and task-state records, with attributed pass/fail/unknown assessments and validity intervals. |
| `AcceptanceRecord` | Bob's authenticated response, bound to the action fingerprint and the offered terms digest. |

The [example's `canonical_effect` and `terms_digest` methods](../examples/v1_task_reassignment.rs) show the encoding. The operation bytes bind `task-7`, Alice, Bob and revision `3`. The terms digest also includes the offered gate-profile version. The host builds these from typed fields and trusted records, rather than accepting an agent's claim that it has permission.

The principal's delegation and Bob's consent answer different questions: **may Alice's agent make this assignment, and has Bob accepted it?** Bob's acceptance does not override a failed delegation check.

## 2. Evaluate the proposal

The host calls `evaluate(&intent, &snapshot)`. With a valid context and no effective revocation, these inputs lead to:

| Inputs | Decision | Next step |
| --- | --- | --- |
| Required conditions pass; Bob has not responded | `DecisionRequired` | Ask Bob to accept the exact offer. |
| Required conditions pass; Bob accepts that offer | `Permitted` | Save the intent, snapshot and decision; then reserve an attempt. |
| Verified delegation check fails; other required checks are resolved | `Prevented` | Do not execute under that delegation. |
| Required task-state evidence is unavailable | `Indeterminate` | Obtain the evidence and evaluate again. |

Suppose the agent now substitutes Carol. The host assigns a new proposal ID. That changes the intent fingerprint, and the new offer requires Carol's consent. Bob's response cannot authorise it. Passing the altered intent to the old `ExecutionAuthority` also fails `check_pre_dispatch`.

For the rest of the example, Bob has accepted and `proposal:42` is permitted.

## 3. Reserve one execution attempt

The host evaluates at sequence `40`. The snapshot is valid through `50`, the evidence used by the decision through `42`, and Bob's acceptance through `45`. The Kernel therefore limits the authority to sequence `42`, inclusive. These are host sequence values, not wall-clock timestamps.

At sequence `41`, the host reserves the attempt in its database. Effective revocation uses the same ordering mechanism:

```text
transaction:
  lock the relevant authority and proposal records
  reject reuse of proposal:42 with a different intent fingerprint
  if an attempt already exists:
    return its saved ticket and outcome

  admission = evaluate(saved_intent, saved_snapshot)
  require admission.decision == saved_decision
  require admission.decision.kind == Permitted
  read current authority, profile, context and sequence
  if relevant condition inputs have changed:
    abort reservation; obtain and save a fresh evaluation before retrying
  require check_pre_dispatch(saved_intent, live_state) to succeed

  save attempt:42/1 with replay key task-reassign:42
  enforce a unique attempt per proposal
commit
```

The saved ticket binds the attempt and replay key to the original intent fingerprint. Two workers claiming `proposal:42` receive the same attempt. The host's `attempt:42/1` and the service's `task-reassign:42` are distinct identifiers; the host stores their relationship.

If revocation wins the transaction order, no new attempt is reserved. If reservation wins, the attempt remains recorded even if authority is withdrawn later. A passing `check_pre_dispatch` alone neither creates nor consumes that reservation.

## 4. Execute exactly the reserved operation

Only the adapter has credentials to call the task service. It loads the saved intent and ticket, reconstructs the `Effect` from the operation it is about to send, and requires an exact match—including profile/version, destination and canonical payload. A ticket for Bob's assignment cannot be used to send Carol's assignment.

For this integration, the task service supports an idempotency key. Its contract must make the task update and its saved result atomic:

```text
service transaction:
  if task-reassign:42 already has a result:
    require the incoming operation to match the saved operation
    return the saved result

  require task-7 is assigned to Alice at revision 3
  assign task-7 to Bob; advance revision to 4
  save the operation and occurrence task-7:revision:4 under task-reassign:42
commit
return the saved result
```

The expected revision is part of the admitted effect. Checking it atomically at the service prevents a concurrent task change from turning this into a different operation. The Kernel cannot perform that resource-side check for the host.

## 5. Recover after the response is lost

The service commits Bob's assignment, but its response never reaches the host. The host still has the original attempt, with an in-flight or uncertain outcome.

| Point in the example | Host record | Task service |
| --- | --- | --- |
| Evaluation at sequence `40` | Saved `Permitted` decision; no attempt yet | Alice, revision `3` |
| Reservation at `41` | `attempt:42/1`, replay key `task-reassign:42` | Alice, revision `3` |
| Service commit; response lost | Original attempt, outcome not yet confirmed | Bob, revision `4`; result saved under the replay key |
| Effective revocation at `43` | New reservations under that authority are blocked | Original result remains |
| Recovery at `44` | Original attempt is confirmed from service evidence | Bob, revision `4`; no second update |

Recovery loads the original intent, snapshot, decision and attempt, and applies the activity's version/migration policy. It reconstructs the original admission from the saved snapshot and checks it against the saved decision. It then queries the saved replay key, or retries the exact operation under that key using the service's idempotency guarantee.

The service looks up the replay key **before** testing revision `3`. It returns the existing revision-`4` result. A retry with a new key would instead be a new operation and would lose this connection to the original attempt.

At sequence `44`, expiry and revocation would block a new reservation. They do not invalidate evidence that the attempt reserved at `41` completed. Recovery is establishing the outcome of that attempt; it does not create new permission.

The host saves an `ActionReceipt` with the original decision and action fingerprint, executor ID, attempt ID `attempt:42/1`, and `Confirmed { occurrence_id: "task-7:revision:4" }`. It calls `validate_receipt` and retains the service evidence supporting that occurrence. Receipt validation checks the records' bindings; the service evidence establishes the result.

If the service cannot reliably identify the result and offers no safe idempotent retry, the host keeps the attempt uncertain. It must not create another attempt merely to obtain a definite answer.

## Checks to carry into your integration

- Changing the assignee or expected revision rejects the old consent and authority.
- Missing required evidence never becomes permission.
- Two workers reserve one attempt and share its replay key.
- Revocation before reservation blocks the attempt; revocation afterward preserves its record.
- A crash after service success recovers the same occurrence without changing revision again.
- An adapter request that differs from the reserved effect is rejected before the service call.

Use the [host-conformance probe](host-conformance.md) for reservation, replay and revocation ordering. Test your service's atomic update, idempotency and crash recovery through its actual adapter. The Kernel's [expiry regression tests](../tests/review_contract.rs) also show why expiry blocks new dispatch without invalidating the receipt for an original attempt.
