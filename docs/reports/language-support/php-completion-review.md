# PHP language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0024
- Reviewed baseline: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
- Last updated: 2026-08-01

## ADR-0020 gate

- [ ] Discovery/config — bounded source/config inventory exists; Composer and
  PHPUnit project profiles are not decoded.
- [ ] Authoritative frontend — stage-3 qualification is incomplete.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `php.phpunit.test_method` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [ ] Correctness, security, completeness, and performance review.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery inventories `.php`, Composer JSON/lock, and PHPUnit XML paths without
executing them. Parsing, dependency resolution, and family support are absent.
The next permitted stage is qualification evidence for `mago-syntax` 1.43.0,
official PHP 8.5.8 `php -n -l`, `nikic/PHP-Parser` 5.8.0 differential behavior,
malformed/resource corpora, five targets, and native OS sandboxes. The current
host has no PHP executable and cannot supply that complete evidence.

Stage 3 must not add a production dependency or worker. Failure to pin or
contain the candidate, differential disagreement, partial-AST anchoring,
uncertain ranges, or repository/dependency execution is `NO_GO`, not permission
to substitute regex or Tree-sitter semantic claims.

## Completion verdict

Not complete. No completion percentage or supported-language count may include
PHP until every checkbox is linked to current-branch evidence.
