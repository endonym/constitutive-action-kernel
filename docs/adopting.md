# Deciding whether to build on the Kernel

Use this guide before you integrate. It describes what changes for an application that puts the Kernel at its action boundary: the problems it closes, the work it hands to your application, how familiar patterns translate, and when it is the wrong tool. Once you have decided, follow the [host integration guide](host-integration.md); this page links to the step that covers each responsibility.

The Kernel does not remove the work of governing actions. It moves that work. Some long-standing problems stop arising, because the contract no longer allows them. Others appear, because the host now owns records and states that a conventional authorisation check never had to keep. Expect both.

## Problems the Kernel closes

- **Permission that widens or transfers silently.** A decision is bound to the exact proposed effect: its profile, operation, destination and canonical payload. Acceptance is bound to the exact terms. Permission for Bob's assignment does not authorise Carol's; a changed effect needs a fresh proposal.
- **Treating "allowed" as "done".** Admission and execution are separate records. A `Permitted` decision does not change the resource; only the adapter does, and the receipt records whether the effect was confirmed, rejected, not dispatched or uncertain.
- **Revocation with an unclear effect.** A revocation request is distinct from an effective revocation. An effective revocation committed before reservation prevents dispatch; one committed later leaves an in-flight attempt and its recorded outcome intact.
- **Missing inputs treated as permission.** An unresolved required condition cannot produce `Permitted`, whatever the other conditions report. A profile with no required conditions must say so explicitly.
- **Disputes about what an action was allowed under.** Each decision fingerprints the intent, profile, evidence, acceptances and revocation state it was judged against, so a later reader can check the decision against the records it cites.

## Responsibilities the host takes on

The Kernel checks the structural consistency of the records it is given. Your application supplies those records and makes the result real.

- **Authenticate every input.** Identity, standing, evidence and acceptance arrive as host assertions. The Kernel binds them; it does not verify their sources. See [step 1](host-integration.md#the-action-path).
- **Handle four outcomes, not two.** `DecisionRequired` and `Indeterminate` are ordinary results. Plan the queue, the user experience and the re-evaluation path for each before you build the happy path. See [step 2](host-integration.md#the-action-path).
- **Persist before you act.** Save the exact intent, snapshot and decision before reserving an attempt, and reserve that attempt durably against current state. See [steps 2 and 3](host-integration.md#the-action-path) and the [conformance probe](host-conformance.md).
- **Keep the capability away from the agent.** The adapter holds the credentials and compares the actual effect with the admitted one. This is a structural decision for the first version, not a later hardening step. See [step 4](host-integration.md#the-action-path).
- **Confirm outcomes from evidence.** Mark an attempt confirmed only when the adapter or external service substantiates it. A lost response stays uncertain until it is reconciled under the same replay identity. See [step 5](host-integration.md#the-action-path) and the [worked example](worked-example.md).
- **Keep records as evidence.** Decisions and receipts are what a later dispute will turn on, so they cannot be migrated away and discarded. Keep them readable for as long as anyone may rely on them, and keep the pinned Kernel revision with them. See [wire and version boundaries](host-integration.md#wire-and-version-boundaries).
- **Own the gap between terms and checks.** `Permitted` means the proposal passed the required checks on the supplied evidence. Natural-language commitments may say more than any check captures. Your activity needs a way to assess what an action meant and to revise its rules. See the [contract](contract.md#purpose-and-claim).

## Translating familiar patterns

| If you would normally use | With the Kernel |
| --- | --- |
| A role or session check before an operation | Required conditions and acceptances evaluated for one exact proposed effect |
| An HTTP 403 | `Prevented`, with reasons recorded in the decision |
| A "pending approval" status | `DecisionRequired`; the approval is an acceptance bound to the exact terms, and a changed proposal needs a new one |
| An error when an upstream service is unavailable | `Indeterminate` when inputs are unresolved or unsupported; nothing is dispatched |
| Retrying a failed request | Replaying the reserved attempt with its original replay identity; no fresh permission is issued |
| Deleting a revoked grant | An effective revocation ordered against reservation; earlier occurrences and their records remain |
| A database migration for a new policy | A new profile version for new proposals; earlier decisions stay bound to the version they cite |

## When not to use it

- **Nothing consequential is mediated.** The Kernel's guarantee concerns effects performed through a controlled adapter. Read-only queries and reversible, low-stakes actions rarely justify a durable decision per action.
- **The agent must hold the credentials.** If the effectful capability cannot be isolated behind a host-controlled adapter, the central guarantee does not hold.
- **You cannot persist durably.** Reservation, recovery and later verification depend on stored intents, decisions and receipts. A host that cannot keep them can use the Kernel only as an advisory check.
- **No one else needs to verify the decision.** If one party sets the rules, executes the action and is the only one who will ever ask what happened, a conventional authorisation layer may be enough.

## What the Kernel leaves outside

The Kernel takes the results of judgements as condition assessments. It does not define how rules are written, interpreted or changed over time, how participants establish standing, or how disputes are resolved. Those belong to the activity and the host. They can use existing institutional records or a dedicated rules layer; the boundary is the same either way.

## Next steps

1. Run the [task-reassignment demonstration](../examples/v1_task_reassignment.rs) and follow the board state alongside each decision.
2. Read the [worked example](worked-example.md) for a lost response and recovery after withdrawal.
3. Follow the [host integration guide](host-integration.md), then run the [conformance probe](host-conformance.md) against your own store and adapter.
4. Use the [contract](contract.md) and [threat model](threat-model.md) as references.
