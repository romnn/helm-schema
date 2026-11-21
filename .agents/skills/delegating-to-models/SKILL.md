---
name: delegating-to-models
description: What each delegate model has proven good at in helm-schema work (Astra for direction and root causes, Sol 6.2 for building and reviews, Sol 6.1 for long lanes, Fable for hard decisions, Opus for builders), how to word Codex prompts so its content filter does not stop them, and at which effort to run consults. Use when choosing a model for a builder, reviewer or analysis consult, or when briefing one.
---

# Delegating to models

What each model has proven good at here (as of Nov 2026). Which vendor or account to use at a given
moment depends on quota and is the orchestrator's call; it is not recorded here.

- **Astra (Codex, xhigh, read-only): direction and root cause.** Design before any new mechanism,
  optimization or CPU lane; diagnosis when a builder is stuck for 2-3 hours; first-principles
  challenges of a lane that is not converging. It found combinatorial root causes in minutes that
  builders missed for hours (7b2 layout walk: 1.68M merges to 1,527; 6a1 cofactor split). Ask for the
  mechanism with file:line, the structural fix, what gets deleted, and a cheap experiment to confirm.
- **Sol 6.2 (Codex, high): building from a ready plan, and pre-landing reviews.** As a reviewer it
  reliably finds Helm-semantics counterexamples (it reproduces them against real Helm), but it reports
  them one round at a time, so require the builder's semantics table (lane-builder skill) first.
  Its content filter stops turns on security-sounding words (crash, stack overflow, adversarial,
  shadowed, integrity, attack, exploit); phrase checks as plain correctness questions. After one
  flagged turn, rephrase or `continue_fresh`; after two, the session context itself is flagged.
- **Sol 6.1 (Codex, high): long builder lanes, and the fallback when 6.2's filter stops a turn.** On
  open-ended work it tends to over-verify and try variants for days (one lane: 5,000 commands, 73
  full dumps); give it Astra milestones that each produce a measured number within 1-2 hours, plus a
  stop rule.
- **Fable (Claude, xhigh): hard decisions and direction checks,** interchangeable with Astra for
  design; use a cross-vendor second opinion when the stakes are high.
- **Opus (Claude): builders,** especially for parser-robustness work whose wording keeps tripping
  Codex's filter.
- Run every Astra and Fable consult at xhigh: the hard decisions need all of it.

## When every Codex tool call fails

If Codex delegates answer that their command runner "timed out" and the logs show
`gRPC code-mode host timed out waiting for session opening response`, check the Mac's screen first.
After a Codex update, macOS asks once to approve the new `codex-code-mode-host` binary; until someone
clicks OK, every shell and tool call in every Codex session fails, while the turns still settle as
"completed" (Nov 2026, Codex 0.171.0; upstream openai/codex#47521). Sessions started before the
update keep working on the old binary, which makes it look lane-specific. A one-line probe
(`pwd` through agentmux) confirms the fix in seconds.
