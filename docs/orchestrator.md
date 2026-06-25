# Kittenscrew Orchestrator — v1

**Approved 2026-06-25.** This document is the canonical, frozen description of the
orchestrator *contract*. Agent internals (how an agent works, what an agent is)
stay pluggable behind the Driver seam and are explicitly **out of scope** here.

---

## North star

- **Code quality is model-bound** — it varies with the model and isn't our moat. Accept it.
- **The moat is lifecycle orchestration**: plan execution, situation assessment, and
  coordination through change (modification, fixes, tests, shifting requirements, surprises).
- **The DAG drives; the model fills leaves.** The harness takes the next ready node,
  dispatches a prompt scoped to only that node, verifies deterministically, and advances.
  **Done = the frontier is green, not "the model said done."**

## The sovereign principle

The orchestrator program is **supreme**; every agent (AI or not) is **subordinate**.

- An agent's only power over the plan is to **REQUEST** a change. It has no direct write.
- Acceptance is governed by deterministic **rules + config** — capability, invariants,
  policy — yielding **accept · reject · escalate**.
- The authoritative state is mutated **only via validated requests** (`spec apply` is the gate).

Freedom to propose, no power to impose. This is how adaptivity (ReAct, surprises) coexists
with determinism: agents flood requests; the sovereign commits only the valid ones.

## Core invariants (the bedrock)

1. **State is verifiable, not asserted.** Every task carries a re-runnable verify method
   (its Definition of Done). No method → not-completable (a *severity-scaled* event).
2. **Done is delegated.** The done-bit is written by a verification authority
   (check / agent / owner), never self-asserted by the executor or the automaton.
   `done` (verified) vs `"done"` (asserted) is a real state distinction.
3. **Separation of powers.** Executor ≠ judge, and the verify artifact is **immutable to the
   executor** (outside its write-scope). The defendant cannot edit the judge's rulebook.
4. **Verify integrity.** A green is authoritative only if its check was unchanged by the
   change it gates. `criterion_tampering` **never passes silently** + mandatory explanation
   + owner judgment — *categorical*, not severity-scaled.
5. **Capability = f(state).** The allowed action-set is derived from plan state; forbidden
   actions are *absent*, not policed. Terminal capabilities (declare-done, ship, push) unlock
   only when the deterministic done-predicate holds.
6. **Nothing dies silently.** Every event surfaces. **Stall** — the orchestrator stops while
   work remains — is the worst failure and *always* escalates.
7. **Failure → plan mutation, not stall.** A bug or surprise converts into re-planning
   (backprop, add node, re-evaluate) — forward motion, never death.
8. **Accountability doesn't transfer.** An agent that delegates or consults still owns its result.
9. **Escalation is bounded.** Local owner first, user (root) last; every escalation carries
   a **deadline + fallback** (no owner-stall — the human is the highest-latency owner).
10. **Cost has three tiers.** plan = expensive · leaf = cheap · orchestration + verify = **zero**
    (deterministic code, no model in the management loop).

## Architecture (behind the Driver seam)

| layer | task | role |
|-------|------|------|
| Driver seam | T60 | backend-agnostic boundary; agents/models plug in here |
| DAG drive loop | T62 | next ready node → scoped prompt → apply → verify → advance |
| Verify (done-oracle) | T63 | per-task runnable method: `check:` / `§V:` / `predicate:` / `owner` |
| Bounded replan | T74 | retry → local-patch → ReAct re-plan episode (bounded valve for the unexpected; output = a plan-mutation *request*) |
| Capability / tripwire | T64 | per-state action table (cage) + residual negative filter |
| Delegation | T77 | SAFE (dep-independent ∧ scope-disjoint) · WORTH-IT · WHO (competence + cheap-tier) |
| Monitoring / events | T78 | post-task hook; events + severity; triggers; integrity guard |
| Bench (weight) | T75/T76 | A/B + orchestration sim — measure management, not code; stall = worst |

**Owner** = `user | check | agent`, recursive on the agent spawn-tree (user is root).
**ReAct** is a bounded, caged valve for the *unexpected* only — its product is a plan-mutation
request, never free action; the deterministic core handles the known (~95%).

## Events

`regression` · `blocking` (blast-radius / frontier-blocked, **not** a global count) ·
`slow_task` (over-estimate **and** no-progress / doom-loop) · `stall` · `significant_change`
(group fan-in gate) · `user_feedback` (+ `repeated_correction` meta) · `missing_verify` ·
`criterion_tampering`.

- **Severity** = deterministic `f(base × blast_radius × value/risk)`, banded. Humans set the
  weights and thresholds; nobody judges a single case ad hoc.
- **Triggers** from three sources: reactive (event) · cadence (every-N-tasks, `report`|`approve`) ·
  milestone (gate).
- `missing_verify` **scales** with task scale/consequence; `criterion_tampering` is **categorical**.

## Status

- **Built & proven (on `main`):** T60 Driver seam, T62 drive loop, T63 verify, T74 bounded
  replan, T75 A/B, T76 orchestration bench (simulated AI, fully offline). 118 tests.
- **Designed & specced (not built):** T64 capability/tripwire, T77 delegation, T78 monitoring.
- **Open edges (deferred, surface in build):** concurrency conflict for *overlapping* parallel
  work, flaky-check runtime policy, event-mid-node preemption, global deadlock (all-waiting-on-
  offline-owner), semantic drift ("the world moved" — a correctly-done node invalidating a
  not-yet-started node's assumptions, softer than a hard regression).

## Open tracks (reserved, not active)

- **Agent internals** — how an agent works (single dispatch? recursive ReAct? mini-kittenscrew?)
  and what it is (competence-scoped specialist? skill subset? remote process?). The *contract*
  is frozen; the *implementation* is pluggable behind the Driver seam.
- **A2A external consultants** — ad-hoc expert agents (research / review / brainstorm) an agent
  calls peer-to-peer; accountability stays with the caller; transport only, our DAG/verify stays
  the brain.
- **Routing server** — provider routing / failover / response-cache / dedupe / ctx-reduction.
  The dockerized CCR + free-model failover is its embryo.

---

**Version:** v1 — orchestration contract frozen 2026-06-25. Internals stay pluggable.
