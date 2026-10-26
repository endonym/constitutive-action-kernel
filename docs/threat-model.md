# Threat model

The Kernel checks proposed actions before your application executes them. An agent can propose a change, but your application—the host—keeps the credentials and access needed to perform it. The host authenticates the required identity and standing, builds an `EvaluationSnapshot`, and calls `evaluate`. Before executing a permitted action, it durably reserves an attempt. The adapter then checks that the operation it will perform matches the admitted action.

## Assets and boundaries

Protect the principal's limits on delegated work, participants' consent and commitments, the rules the action must satisfy, confidential inputs, access to the resource, and the record of what was proposed, permitted and executed. The relevant boundary is between **agent-controlled** text or tools and **host-controlled** authority. Unresolved required inputs cannot authorise execution. The host saves execution attempts durably and checks uncertain outcomes against the adapter or external service. It also controls disclosure of confidential inputs and records.

An agent can submit arbitrary proposals and prompt content, repeat an old proposal, alter a recipient or payload after approval, exploit a stale snapshot, or claim acceptance or standing it does not have. The kernel rejects structural inconsistencies in supplied records. The host authenticates their sources and isolates the effectful capability. A malicious host that controls both evidence and capability falls outside that trust boundary; deployments can add independent attestations, signatures or audits when they need to constrain host power. 

## Required invariants

- **Keep the action unchanged.** An effect-changing parameter, destination, principal, context or governing version cannot change under an existing admission. A profile defines how to encode the effect consistently; the adapter compares the actual effect with the admitted digest or typed parameters.
- **Check required evidence.** Required evidence must be present, verified, current and structurally consistent. The host resolves conflicting assessments or marks the condition unknown. Execution authority expires no later than its snapshot, used evidence or required acceptances. Profiles without required conditions must explicitly select `ExplicitOpen`.
- **Bind consent to the proposal.** Host policy identifies who must accept the proposed terms. Their authenticated response is bound to the exact proposal and consequential terms digest. Silence is pending. The host includes the originating delegation’s constraints among the required conditions; acceptance does not override them.
- **Order revocation and reservation.** A revocation *request* is routed to an authorised decision process. Only an effective revocation or an explicitly authorised suspension changes admission. Effective changes and dispatch reservation have a defined order; a later decision does not retroactively erase an in-flight effect.
- **Preserve retry identity.** The host makes one durable reservation for one admitted effect and idempotency identity. Recovery retains the original attempt and idempotency identity. Retries reuse that identity or reconcile the original operation rather than create a second occurrence. Mark an attempt confirmed only after checking evidence from the adapter or external service that identifies what happened.
- **Record what happened.** Keep the decision, execution attempt and result distinguishable from any later assessment of that result or whether it fulfilled an obligation. `ActionReceipt` records the execution outcome; the host keeps the other records with their authorship.

## Kernel and host responsibilities

The v1 API binds the exact action, profile, evidence, acceptance and revocation records and checks their structural consistency. The host authenticates inputs, supplies all required records, durably reserves an execution attempt, controls access to the resource and checks the outcome. TGC-Demo runs the [host-conformance probe](host-conformance.md) against its durable local host. The Kernel’s reference probe implementation and standalone task-board demonstration use in-memory state.

The activity's rules and participants determine how to interpret natural-language terms. The host supplies those assessments to the Kernel as condition results. The execution record preserves the decision and outcome.
