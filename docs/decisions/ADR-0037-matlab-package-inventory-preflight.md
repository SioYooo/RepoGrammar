# ADR-0037: MATLAB R2024b+ package inventory preflight

- Status: Accepted
- Date: 2026-08-01
- Scope: MATLAB in ADR-0020; bounded package inventory, not language completion
- Related: ADR-0020, ADR-0030, `docs/reports/language-support/matlab-completion-review.md`

## Context

MATLAB source, project files, toolbox archives, and licensed analysis products
do not share one redistributable, non-executing frontend. MathWorks documents
that projects can run startup/shutdown code and change the MATLAB path, and that
the XML under `resources/project` has a format subject to change. Neither
opening a project nor interpreting that XML is an acceptable default.

MathWorks separately documents `resources/mpackage.json`, introduced in
R2024b, as a JSON package definition containing required package identity and
an optional dependency array. Dependency entries carry a MATLAB identifier,
compatible-version text, and required UUID. This is a suitable bounded static
inventory input.

References, retrieved 2026-08-01:

- <https://www.mathworks.com/help/matlab/ref/mpackage.json.html>
- <https://www.mathworks.com/help/matlab/matlab_prog/create-projects.html>

## Decision

RepoGrammar adds stable `matlab` and `matlab-config` tokens. Normalized
lowercase `.m` paths are metadata-only discovery; their bytes are never passed
to a parser. Extension recognition does not distinguish MATLAB from every
other language that uses `.m` and therefore proves no dialect or support.

Only exact root/nested `resources/mpackage.json` is parsed. The in-process
reader:

- enforces the shared duplicate-key JSON gate, depth/member/key limits, the
  default one-MiB input bound, and a 2,000-entry dependency ceiling;
- requires a bounded ASCII MATLAB-identifier subset, bounded non-path ASCII
  root version text, UUID identity, and three-part schema version;
- qualifies schemas 1.0.0 and 1.1.0; later syntactically valid schema versions
  retain an `InsufficientSupport` UNKNOWN and produce no dependency rows;
- records dependencies as `matlab_add_on` identities in `name@uuid` form,
  optional compatible-version text as a requirement, directness `direct`,
  scope `unknown`, and evidence `manifest_declared`;
- rejects URL/path-shaped version text, malformed identities, ambiguous JSON,
  conflicts, unsupported fields, and resource overflow with a claim-scoped
  `matlab_dependency_inventory` UNKNOWN; and
- ignores provider/contact/display fields so they do not enter output.

The reader never invokes MATLAB or Octave; opens a project; evaluates code;
loads packages, MEX, Java, apps, or P-code; installs a toolbox; mutates the
MATLAB path; accesses the network; or includes Simulink.

## Consequences

MATLAB remains `discovered_only` and unsupported under ADR-0020. There is no
MATLAB source frontend, Code Analyzer integration, authoritative provider,
release-selected dialect, source code unit, exact-anchor family,
`matlab.unittest` contract, Simulink analysis, or licensed-provider claim.
Missing licensed tooling is not reported as `LICENSE_BLOCKED` because this
change did not probe a licensed installation; the semantic provider is simply
not integrated (`PROVIDER_UNAVAILABLE`).

## Follow-up

Qualify a redistributable parse-only frontend or an explicitly enabled,
license-aware, sandboxed MATLAB provider over supplied bytes. It must prove
release/version behavior, parse degradation, output limits, no path mutation,
and no execution before any source or family gate can advance.
