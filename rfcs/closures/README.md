# Closure Records

This directory holds **closure records**: the backward-looking counterpart to
`rfcs/handoffs/`.

> `handoffs/` is the forward instrument — what to build, before it exists.
> `closures/` is the backward instrument — what was observed, after it
> shipped.

The project had the first and not the second, which was RFC-063's diagnosis
of how nine RFCs reached `rfcs/done/` carrying a `Status: Implemented` that
was not true (the 2026-09-01 external audit). A closure record makes that
class of error mechanically hard: `scripts/check-rfc-lifecycle.sh` refuses to
let a new RFC enter `done/` without one (Task 038).

## Convention

- One closure record per RFC that closes, named `NNN-slug.md` — the exact
  same basename its RFC has in `rfcs/done/` (RFC-063 §6.3: "the record's id
  and slug match the RFC's").
- **One entry per numbered acceptance criterion**, verbatim from the RFC's
  own `## N. Acceptance Criteria` section — naming what was run, what was
  observed, and where it was verified. A criterion with nothing to put in
  *what was observed* is not a criterion (RFC-063 §6.1) — say so rather than
  inventing an observation.
- A **"Criteria not met, and why this closes anyway"** section, required
  even when empty ("None."), per RFC-000's granularity clause and RFC-063
  §6.1 — this is the section that keeps a partial ship a written statement
  instead of a silent omission.
- References to material outside `rfcs/` (a task file, a review, a CI run)
  are **plain text, never markdown links**, when the target is not
  git-tracked — `.git-exclude/` in particular. `rfcs/` ships in the release
  archive; `.git-exclude/` does not, so a link there is dead for every
  reader outside this working copy.
  `scripts/check-rfc-lifecycle.sh` enforces this for every link inside
  `rfcs/`; it does not know which targets are untracked, so this is still on
  the author.
- See `037-source-lifecycle-refresh-policy-and-change-detection-ux.md` for
  a worked example, and
  `045-search-in-folder-flow-and-friendly-folder-management.md` for the
  shape a partial closure ("criteria not met" non-empty) takes.

## Legacy allowlist

`LEGACY-ALLOWLIST.txt` exempts the RFCs already in `done/` before RFC-063
existed from this requirement — see that file's own header for the
shrink-only rule that keeps the exemption finite rather than a permanent
grandfather clause.
