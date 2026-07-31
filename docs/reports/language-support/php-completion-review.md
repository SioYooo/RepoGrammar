# PHP language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0024
- Reviewed baseline: `0472e6bf0fa74a37a0bb9e43eb878f2c04185c22`
- Last updated: 2026-08-01

## ADR-0020 gate

- [ ] Discovery/config — bounded source/config discovery and a static Composer
  dependency inventory exist; selected Composer/PHPUnit project profiles are
  not decoded or qualified.
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

Discovery inventories `.php`, Composer JSON/lock, and PHPUnit XML paths. Exact
`composer.json` and `composer.lock` basenames additionally enter a bounded,
unique-member static parser that emits ADR-0030 `composer` dependency records
and `php_dependency_inventory` typed UNKNOWN without executing PHP, Composer,
autoloaders, plugins, scripts, PHPUnit, repository code, or dependencies.
Manifest `require`/`require-dev` records retain direct runtime/development
scope; lock entries remain indirect `lockfile_resolved` evidence and explicitly
do not prove installation, runtime selection, or manifest coherence. Platform
packages, virtual relations, malformed/duplicate/unsupported input, conflicts,
and bounds are fail-closed.

This slice creates project-config units only for dependency evidence and does
not select a Composer/PHPUnit project profile, parse PHP source, expose package
source in ordinary product output, or create a PHP/PHPUnit family claim. PHP
therefore remains `discovered_only`. The next semantic stage remains
qualification evidence for `mago-syntax` 1.43.0,
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
