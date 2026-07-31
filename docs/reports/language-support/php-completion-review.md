# PHP language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0024
- Dependency prerequisite: `f27812bf79c1eb3c5eca8179b17285794b344491`
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
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery inventories `.php`, Composer JSON/lock, and PHPUnit XML paths. Exact
`composer.json` and `composer.lock` basenames additionally enter a bounded,
unique-member static parser that emits ADR-0030 `composer` dependency records
and `php_dependency_inventory` typed UNKNOWN without executing PHP, Composer,
autoloaders, plugins, scripts, PHPUnit, repository code, or dependencies.
Manifest `require`/`require-dev` records retain direct runtime/development
scope; lock entries retain unknown directness and `lockfile_resolved` evidence and explicitly
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

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | PHP / 14 |
| Dialect/version | Inventory accepts lowercase `.php`; no selected PHP profile. PHP 8.5.8 is a future differential oracle identity, not current source support. |
| Provider/frontend/version | None integrated. `mago-syntax` 1.43.0, `nikic/PHP-Parser` 5.8.0, and PHP 8.5.8 remain qualification candidates only. |
| Discovery/config | PHP source, exact Composer manifest/lock names, and PHPUnit XML inventory with PHP-specific exclusions. |
| Manifest/lockfile | Bounded `composer.json` declarations and `composer.lock` pins; platform/virtual packages and coherence/effective profile remain unresolved. |
| Owned source IR | Absent; only config units/IR exist. |
| External symbols | Absent; autoloading, namespaces, class ownership, and dependency-to-import binding are unresolved. |
| Library Contracts | Exact-version registry exists, production packs = 0; no PHP contract is usable without provider-resolved symbols. |
| Exact-anchor family | Absent; `php.phpunit.test_method` remains proposed. |
| Fixtures | Strong Composer malformed/conflict/resource/leakage/incremental coverage; no qualified PHP source/family matrix. |
| Primary UNKNOWN cases | Composer coherence, platform/virtual relations, PHP profile, autoload graph, source recovery, attributes, generated code, PHPUnit version/runner semantics, and provider availability. |
| Source-free result | Pass for static inventory and product leakage tests; no claim-bearing PHP readiness surface. |
| Completion state / counted | `discovered_only`; strict gate count `1/9`; Top-20 complete = no. |

## Four-part review

- Correctness: accepted Composer rows preserve declaration versus lock evidence;
  they do not select a project, installed graph, PHP version, or PHPUnit runner.
- Security: duplicate-key/resource checks and static parsing prevent PHP,
  Composer, plugins, autoloaders, scripts, tests, repository code, dependencies,
  children, or network access. Native frontend isolation remains unproven.
- Completeness: inventory is useful, but frontend/IR, provider/UNKNOWN closure,
  exact family, completion fixtures, and linked final audit are open.
- Performance: JSON/record/input ceilings are tested; no admitted frontend or
  large Composer/autoload graph benchmark exists.

Evidence paths are `src/rust/adapters/languages/php.rs`,
`src/rust/adapters/parsing/php.rs`, product/incremental tests, ADR-0024, and
prerequisite `f27812bf79c1eb3c5eca8179b17285794b344491`. Exact non-claim:
Composer presence or a locked version does not prove installation, autoload
identity, PHPUnit behavior, API compatibility, or family membership.
