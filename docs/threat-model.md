# Threat model

The kernel governs a *mediated consequential effect*. An agent forms a proposal while the host retains the credential, storage mutation or network capability that makes the effect real. The host supplies a versioned evaluation snapshot, the kernel returns an exact-action gate decision, and an adapter holding the capability executes only a properly reserved admission. The host supplies the authenticated identity and standing required by the activity.

## Assets and boundaries

The assets are the principal's bounded delegation, other participants' acceptance and commitments, the integrity of applicable conditions, confidential inputs, the adapter's effectful capability, and an auditable record connecting proposal, decision, dispatch and occurrence. The relevant boundary is between **agent-controlled** text or tools and **host-controlled** authority. Unresolved required inputs cannot authorise execution. The host persists admitted attempts and reconciles uncertain execution outcomes. The host controls disclosure of confidential inputs and records.

An agent can submit arbitrary proposals and prompt content, repeat an old proposal, alter a recipient or payload after approval, exploit a stale snapshot, or claim acceptance or standing it does not have. The kernel rejects structural inconsistencies in supplied records. The host authenticates their sources and isolates the effectful capability. A malicious host that controls both evidence and capability falls outside that trust boundary; deployments can add independent attestations, signatures or audits when they need to constrain host power. 

## Required invariants

- An effect-changing parameter, destination, principal, context or governing version cannot change under an existing admission. A profile defines canonical effect encoding; the adapter checks the actual effect against the bound digest or typed parameters.
- Required evidence must be present, verified, current and structurally consistent. Host assessors resolve substantive conflicts or mark the condition unknown. Execution authority expires no later than its snapshot, used evidence or required acceptances. Profiles without required conditions must explicitly select `ExplicitOpen`.
- The person asked to accept an undertaking is identified by host policy. Their authenticated response is bound to the exact proposal and consequential terms digest. Silence is pending. The host includes the originating delegation’s constraints among the required conditions; acceptance does not override them.
- A revocation *request* is routed to an authorised decision process. Only an effective revocation or an explicitly authorised suspension changes admission. Effective changes and dispatch reservation have a defined order; a later decision does not retroactively erase an in-flight effect.
- The host makes one durable reservation for one admitted effect and idempotency identity. Recovery retains the original attempt and idempotency identity. Retries reuse that identity or reconcile the original operation rather than create a second occurrence. Confirmation requires reconciled adapter or external evidence.
- The wider evidence record distinguishes a gate decision, a dispatch attempt, technical occurrence, interpretation of an outcome and fulfilment of an obligation. The current `ActionReceipt` represents the technical outcome, not the whole record.

## Kernel and host responsibilities

The v1 API binds the exact action, profile, evidence, acceptance and revocation records and checks their structural consistency. The host establishes input authenticity and completeness, reserves one dispatch durably, controls the effect capability and reconciles the outcome. TGC-Demo runs the [host-conformance probe](host-conformance.md) against its durable local host. The Kernel’s reference probe implementation and standalone task-board demonstration use in-memory state.

Activity governance supplies substantive judgements about rules and natural-language interpretations. For mediated effects, governing conditions determine whether an action crosses into execution, and the record shows the decision and outcome.
