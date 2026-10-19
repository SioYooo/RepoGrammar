# Anchor role rows (Phase 2b)

Status: **PREREGISTERED**. Base: the Phase 2a commit `eb50101` on
`feat/tree-sitter-frontends`. Local only; no push or merge. Context:
[binding engine](binding-engine.md) (Phase 2a, `QUALIFIED`), ADR-0057 and
ADR-0058.

## Question

How much of a lane's role decision (which anchor applies to a declaration)
can be expressed as data rows over the shared binding engine, with output
byte-identical? This is a measurement, not a threshold test. Precedence,
context propagation and conditional UNKNOWNs stay explicit Rust that reads
the rows. If the code that reads the rows starts replaying the lane's control
flow, that is the finding.

## Scope

C# only, in `adapters/parsing/csharp.rs`:

- class-like roles: ASP.NET Core controller (attribute or base),
  EF Core `DbContext` and FluentValidation validator;
- method roles: xUnit, NUnit and MSTest test attributes. ASP.NET Core actions
  already read the `HTTP_ROUTE_ATTRIBUTES` table.

The EF Core `DbSet` entity set (a property-type shape) and minimal-API routes
(receiver chain) are declared as code, not rows, before the experiment.

## Measurement

- Roles whose positive condition is a row: N of the 9 C# roles.
- Lines of role-decision code before and after (`scan_class_like`,
  `scan_method`, `test_method_anchor`), split into row data and remaining
  logic.
- The size of any row-reading code added.

## Oracle

Same as Phase 2a: zero capture mismatches over 179 roots;
`sync-equivalence` 14/14; `product-eval` equal apart from timing, clock and
commit fields; identical parse snapshots of the 4,818-file Unity corpus and
the committed C# fixtures; all C# unit tests passing. Any difference stops the
experiment. The outcome is `QUALIFIED` with measured coverage, or `NO_GO`
with the construct that could not be expressed.
