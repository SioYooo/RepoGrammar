# ADR-0024: PHP sandboxed frontend and PHPUnit preflight

- Status: Accepted
- Date: 2026-07-16
- Scope: PHP in ADR-0020 wave N1; preflight plus discovery/configuration
- Refines: ADR-0004 and ADR-0020
- Related: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/indexing-pipeline.md`, and
  `docs/reports/language-support/php-completion-review.md`

## Context

ADR-0020 requires discovery/configuration, an authoritative frontend,
RepoGrammar-owned IR, typed uncertainty, an exact-anchor family, source-free
product readiness, independent review, and a linked completion audit before a
new language is supported. PHP now has only the bounded discovery/configuration
module described below; every semantic and support gate remains open.

PHP requires a deliberately split authority. The official PHP implementation
is the syntax oracle, but running even `php -l` executes a native interpreter.
The mature `nikic/PHP-Parser` exposes a useful AST and byte positions, but its
5.8.0 tag is unsigned, its Packagist dist metadata publishes no checksum, and
the worker would execute PHP plus trusted parser/autoloader code. Rust-native
frontends avoid that runtime in the product path, but still require artifact,
conformance, malformed-input, range, diagnostic, and resource qualification.

The dated preflight snapshot is:

- PHP 8.5.8, released 2026-07-02, tag `php-8.5.8`, commit
  `26b97507444c4fbda072f57dda1820f7b7d5e467`, official `.tar.xz` SHA-256
  `58910198d19e873048fe87cdfe16bc790025417ede3d1651bfa1c4b533d573f2`;
- Mago 1.43.0, verified signed release tag at commit
  `3bee8a65ba55d037f8a9656e7b6be029f817c4e3`, with candidate
  `mago-syntax` crate SHA-256
  `098297d346a0d3ef68c2996628350d7ea589448eddeccfc85086327de0d54a4b`,
  license `MIT OR Apache-2.0`, and MSRV 1.96.0;
- `nikic/PHP-Parser` 5.8.0, commit
  `044a6a392ff8ad0d61f14370a5fbbd0a0107152f`, BSD-3-Clause;
- Tree-sitter PHP 0.24.2, verified signed release tag at commit
  `5b5627faaa290d89eb3d01b9bf47c3bb9e797dea`, crate SHA-256
  `0d8c17c3ab69052c5eeaa7ff5cd972dd1bc25d1b97ee779fec391ad3b5df5592`,
  MIT;
- Composer 2.10.2, verified signed release tag at commit
  `8d4439f572a97670a9edc039eb3b093cc976b4bc`, MIT, with
  `Composer\Package\Locker::getContentHash()` at that exact source identity as
  the only accepted initial lock-content-hash algorithm authority; and
- PHPUnit 13.2.4, commit `8f5180f4627fc1978be2f61d8d9979dbe37e0c10`,
  current on 2026-07-16 and requiring PHP 8.4.1 or newer.

These identities are research inputs. No crate, PHP runtime, Composer package,
worker image, binary, or checksum is admitted to RepoGrammar by this ADR.

## Decision

### D1. PHP is discovered only and unsupported

The bounded discovery/configuration module adds stable `php` and `php-config`
inventory tokens plus PHP-specific path exclusions. Full and incremental
indexing persist only bounded source-free file metadata and expose ordinary
repository-level inventory reporting. This advances PHP only to
`discovered_only`; extension or manifest recognition is not language support.

There is still no PHP parser, worker, production dependency, project model,
code unit, IR, semantic fact, typed `UNKNOWN`, PHPUnit family, PHP-specific
readiness promotion, or support behavior. The candidate frontends and runtime
identities in this ADR remain research inputs, not admitted dependencies or
executed product paths.

### D2. Frontend authority is qualification-first and fail-closed

The candidate production frontend is `mago-syntax` 1.43.0 inside a separately
reviewed RepoGrammar worker. It is preferred for qualification because it is a
current Rust-native syntax layer with signed release provenance and a published
crate checksum, and it avoids a default host-PHP dependency. It is not yet an
accepted production dependency: Mago's active AST/CST/HIR restructuring,
transitive crates, public API stability, diagnostic semantics, range contract,
and resource behavior remain open gates.

Qualification must compare the candidate against two isolated references over
a committed corpus:

1. official PHP 8.5.8 CLI syntax validity using `php -n -l` in an OS sandbox;
2. `nikic/PHP-Parser` 5.8.0 configured explicitly for PHP 8.5 as the mature AST
   and location differential.

Neither reference is silently promoted to the product path. The PHP-Parser
fallback requires its own immutable vendored artifact/digest, minimal PHP CLI
runtime, exact extension handshake, five-target packaging, sandbox, and native
runtime evidence. Its unsigned tag, blank dist checksum, and best-effort newest
version support block admission until replaced by RepoGrammar-owned intake
evidence. The PHP 8.5.8 patch version is an oracle/runtime identity; the parser
profile is the PHP 8.5 language minor. They must not be conflated.

Tree-sitter PHP 0.24.2 may later be a version-pinned tolerant syntax fallback
and candidate generator. `ERROR`, `MISSING`, or otherwise recovered trees never
support a family. Its generated C parser/scanner and `cc` build path require
the same supply-chain and five-target proof as other native grammar crates.

No frontend is a PHP semantic oracle. RepoGrammar-owned classifiers decide
exact bounded claims, and unsupported runtime behavior remains `UNKNOWN` or a
non-claim.

### D3. Every parser path is process-isolated

All future PHP frontends consume supplied bytes and one normalized allowlisted
PHP profile summary through the versioned semantic-worker boundary. Raw
Composer JSON/lock and PHPUnit XML never enter a frontend worker. The default
process contract is fail-closed:

- one worker process and one supplied PHP file per request;
- no repository current working directory or repository/home/credential/cache
  mount; only the immutable worker/runtime artifacts and an empty private
  temporary directory are visible;
- no network, child process, inherited handles, ambient configuration, or host
  writes;
- a minimal allowlisted environment, with no Composer, PHP, framework, proxy,
  credential, or package-manager variables;
- source arrives through a bounded framed protocol, never by worker traversal;
- stdout and stderr are drained concurrently, bounded, and sanitized;
- timeout, CPU, memory, node, depth, diagnostic, output, crash, signal, panic,
  protocol, or range failure invalidates the whole response; and
- if a claimed target cannot enforce filesystem, network, descendant-process,
  time, CPU, memory, and output limits, PHP semantic capability is unavailable.

The PHP reference/fallback profile additionally requires CLI-only `php -n`,
JIT/opcache/FFI and URL access disabled, an exact allowlist of required
extensions, and no target Composer/autoloader/bootstrap. The worker may execute
its immutable trusted PHP-Parser library; it must never execute target PHP.

Default analysis must not run PHP, Composer, PHPUnit, Artisan, a vendor binary,
autoload/bootstrap files, plugins, scripts, extensions selected by the target,
tests, generated proxies, `include`, `require`, `eval`, reflection, or network
package resolution.

### D4. Discovery and project inventory are source-free and nonexecuting

The discovery module uses stable inventory-only tokens:

- `php` for an exact case-sensitive `.php` suffix, including basename `.php`;
- `php-config` for exact root/nested basenames `composer.json`, `composer.lock`,
  `phpunit.xml`, and `phpunit.xml.dist`.

Configuration classification precedes source suffix classification. `.inc`,
`.phtml`, `.phpt`, `.php.dist`, extensionless `artisan`, `composer.phar`, and
`auth.json` are not N1 inventory. Exact `vendor` remains globally excluded.
PHP candidates below exact `.composer` or `.phpunit.cache` components receive a
PHP-only `language_specific_exclusion`; those directories must not be globally
pruned for unrelated languages.

Discovery stores only bounded repo-relative path, strict raw-byte hash, size,
and token. It does not decode or parse inventory-only bytes. Full and
incremental indexing classify both tokens as inventory-only before any parser-
facing source-store read, emit at most one deterministic path-free unsupported
warning per accepted token, and create no unit, IR, fact, typed `UNKNOWN`,
family, project-model record, or readiness/support claim. PHP-only generations
are `file_manifest_only`; mixed generations retain
`syntax_only_code_units`. Inventory add/modify/remove and unchanged rounds stay
incremental while PHP is absent from `ParserProjectContext`, and generation
copy-forward purges legacy claim-bearing PHP records while preserving file
metadata. Autosync fingerprinting uses the same classifier and retains its
generic Git-independent conservative charging.

A later separate,
non-executing bounded project-model parser may receive raw configuration bytes
and emit only an allowlisted normalized profile summary:

- from `composer.json`: PHP constraint, PHPUnit dev constraint,
  `config.platform.php`, literal `config.vendor-dir`, and the presence—not
  contents—of autoload, plugin, and script surfaces;
- from `composer.lock`: exact `phpunit/phpunit` package/version plus content-hash
  coherence computed exactly from Composer 2.10.2
  `Composer\Package\Locker::getContentHash()` and proven by committed
  differential fixtures; and
- from PHPUnit XML: schema/profile, an optional literal cache-directory path,
  and only the presence of bootstrap, extension, testsuite, and selection
  settings, with DTDs, entities, XInclude, and network disabled.

Repository URLs, credentials, `auth.json`, script bodies, arbitrary `extra`,
unknown fields, source bytes, absolute paths, and free-form XML values must not
reach CLI/MCP output. `config.platform.php` is intended dependency-resolution
metadata, not proof of an executing PHP runtime. XML/bootstrap/suite inventory
is not proof that a test is loaded, selected, runnable, or passing.

Custom vendor/cache paths are accepted only as literal normalized repo-relative
paths under the validated project root. Absolute, home-relative, environment-
expanded, wildcarded, escaping, malformed, oversized, or conflicting paths
produce `php_composer_project_scope`; they never cause traversal. Their raw
values never leave the project-model parser or reach CLI/MCP output.

PHP admission remains two-phase. Discovery may inventory source/configuration
metadata, but a bounded selected project profile and exclusions must be applied
before any PHP source-store read, frontend request, code-unit creation, or claim
copy-forward. A validated custom `vendor-dir` is used only as a prefix exclusion
over already discovered paths; it is never traversed. Configuration add/modify/
remove invalidates the profile, reclassifies affected PHP paths, and purges
claim-bearing copy-forward rows before analysis resumes. Ambiguous or resource-
exhausted selection leaves semantic capability unavailable rather than parsing
potential dependency code.

### D5. The first exact family is direct PHPUnit test methods

The first family token is `php.phpunit.test_method`. It denotes a source-visible
direct declaration under the pinned PHPUnit 13.2 profile. An anchor must satisfy
all of these conditions:

1. one concrete named class is declared in the current file;
2. its immediate base resolves exactly, within the same namespace block, to
   `PHPUnit\Framework\TestCase` through a fully qualified name, one unambiguous
   ordinary import, explicit alias, or exact grouped import;
3. the method is declared directly in that class and is public, non-static,
   non-abstract, zero-parameter, and explicitly returns `void`; and
4. either the method name has the exact lowercase `test` prefix, or exactly one
   method attribute resolves to `PHPUnit\Framework\Attributes\Test` through the
   same bounded FQN/import/alias/group-import rules; and
5. every method attribute that resolves to that `Test` identity has zero
   positional or named arguments and appears at most once.

The class name need not end in `Test`, and the filename need not end in
`Test.php`; those are suite-selection concerns. Exact prefix and exact `Test`
attribute are compatible variations. Docblock-only `@test`, lookalike imports,
ambiguous aliases, indirect Laravel/application test bases, inherited or trait-
supplied methods, anonymous/abstract classes, non-public/static/abstract
methods, parameterized methods without provider metadata, and non-void methods
do not anchor the first slice. An exact `Test` attribute with any argument or a
duplicate exact `Test` attribute is a blocking attribute-shape obligation even
when another branch of condition 4 would otherwise match.

The compatible PHPUnit profile is mechanically narrow. The selected Composer
project must have one `composer.json`, one coherent same-root `composer.lock`,
and at most one selected PHPUnit XML file (`phpunit.xml` before
`phpunit.xml.dist`). The locked `phpunit/phpunit` version must be exactly in the
13.2 series, the root constraint must admit that exact lock version, and the
selected PHP 8.5 syntax profile must satisfy the bounded root/platform PHP
constraints without conflict. `composer.json` intent or XML alone is auxiliary;
an absent, stale, malformed, algorithm-unproven, ambiguous, or conflicting
lock/profile leaves `php_composer_lock_coherence` or `php_phpunit_version`
unresolved and blocks the PHPUnit family.

A family requires at least three fresh, same-language, compatible exact anchors
under one PHP/PHPUnit profile and no claim-relevant blocking PHP `UNKNOWN`.
Three declarations do not suffice when project/profile evidence is missing,
stale, conflicting, parse-degraded, or insufficient.

### D6. Evidence ladder and typed uncertainty

The evidence order is:

1. **Primary:** fresh output from the qualified sandboxed frontend over supplied
   bytes, clean against the official PHP/PHP-Parser differential corpus, with
   path/hash/range, parser/protocol/profile/artifact, request, and sandbox
   provenance.
2. **Claim-supporting derived fact:** a RepoGrammar-owned exact PHPUnit anchor
   created only after namespace, ancestry, method, project-profile, freshness,
   and claim-impact gates pass.
3. **Auxiliary:** extension/config inventory, Composer-declared constraints,
   PHPUnit XML, Tree-sitter recovery output, and unselected project roots.
4. **Forbidden:** regex/text-only matching, extension-only recognition,
   unpinned parser output, partial/recovered AST anchors, target execution,
   runtime test results, or structural similarity without exact identity.

The first PHP registry must define these initial claim impacts and exact
resolution evidence. These are normative future mechanisms, not implemented
public reason codes:

| Mechanism | Initial claim impact | Exact resolution evidence |
|---|---|---|
| `php_frontend_availability` | Blocks every semantic claim for the affected request/profile when the qualified artifact, target, sandbox, or handshake is unavailable. | One fresh successful request from the exact qualified artifact/target/sandbox identity. |
| `php_resource_protocol` | Blocks every claim from a request after limit exhaustion, truncation, timeout, crash, panic/signal, invalid range/hash, malformed frame, version mismatch, or extra output; never becomes `no_family`. | A new within-limit complete request/response with exact hash, ranges, protocol version, and no discarded output. |
| `php_syntax_profile` | Blocks all anchors in the affected file for absent/conflicting/unsupported profile, any parse diagnostic/recovery, invalid range, or frontend/oracle disagreement. | One selected PHP 8.5 profile plus clean qualified output and passing official differential evidence for the relevant syntax shape. |
| `php_composer_project_scope` | Blocks every PHP family claim in an ambiguous, malformed, oversized, escaping, or unselected project scope. | One bounded root/profile selection with all literal exclusions validated and applied before source admission. |
| `php_composer_lock_coherence` | Blocks every project-profile-derived family claim when lock coherence is absent, stale, conflicting, or algorithm-unproven. | Exact Composer 2.10.2 content-hash reproduction over bounded `composer.json` content plus committed differential fixtures. |
| `php_phpunit_version` | Blocks every PHPUnit family claim in the affected project. | One coherent lock with exact `phpunit/phpunit` 13.2.x, a root constraint admitted under separately qualified Composer 2.10.2 constraint semantics, compatible PHP 8.5 root/platform constraints, and no conflicting selected XML profile. |
| `php_namespace_binding` | Blocks the affected class/method identity. | One unique exact FQN/import/alias/group-use binding in the same namespace block with no local shadow or conflict. |
| `php_phpunit_testcase_ancestry` | Blocks the affected class anchor. | Its immediate base resolves exactly to `PHPUnit\Framework\TestCase`; indirect/application ancestry remains outside N1. |
| `php_phpunit_test_attribute_identity` and `php_phpunit_test_attribute_shape` | Block the affected method when attribute identity is needed or an exact `Test` attribute is malformed. | One unique exact `PHPUnit\Framework\Attributes\Test` binding, zero arguments, at most one occurrence, and no conflicting lookalike. |
| `php_phpunit_data_provider_obligation` and `php_phpunit_dependency_obligation` | Block the affected method when `DataProvider*`, `TestWith*`, `Depends*`, parameters, or producer semantics are present. | N1 resolves only by proving those shapes absent from the zero-parameter direct method; supporting them requires a successor slice. |
| `php_trait_test_origin` | Trait-supplied tests are excluded and do not block an unrelated direct method; adaptations, aliases, or collisions that can alter the direct method block that method. | Exact local class inventory proves the direct declaration is unaffected; trait-origin test support is deferred. |
| `php_dynamic_include_autoload` and `php_runtime_mutation` | Block only class/method identity or reachability claims intersected by a source-visible dynamic load, `class_alias`, conditional declaration, `eval`, magic, reflection, or service-container mutation. Otherwise they are non-blocking context and never prove runtime behavior. | Source-backed local evidence proves no intersecting mechanism; runtime selection/execution remains a non-claim. |
| `php_generated_source` | Blocks an affected anchor only when an allowlisted exact generated-file/region header, a bounded selected generator-output mapping, or an authorized RepoGrammar-owned path+hash provenance receipt positively applies or conflicts. Filename suspicion and absence of a marker alone are neither provenance nor blockers. | Exclude the positively generated file/region, or supply fresh exact path+hash evidence that the signal is stale or does not apply. Conflicting positive signals remain blocking; no source type silently overrides another. |
| `php_phpunit_suite_selection` | Does not block the source-visible direct-method family. It blocks only selected/runnable/pass-fail subclaims, which N1 does not make. | A future executable-suite model would be required; XML presence alone never resolves it. |

One authoritative PHP claim-impact classifier must decide blocking,
non-blocking, compatibility, readiness, and recovery effects. Callers may
route, persist, count, or format the result but must not reimplement it from raw
reason/claim strings. Dynamic includes, magic methods, service-container
resolution, Laravel runtime routes, generated proxies, runtime mutation, and
test execution remain non-claims for this family.

### D7. Resource and platform qualification

Discovery inherits the shared exact ceilings: 1 MiB/file, 100,000 accepted
files, 512 MiB accepted bytes, 100,000 reported skips, 250,000 visited entries,
and depth 256.

The wire encoding is one LF-terminated UTF-8 JSON record. Raw source and
configuration byte fields use standard padded Base64; encoded caps include the
entire JSON envelope and terminating LF. Initial frontend-worker ceilings, to
be lowered or retained by corpus/fuzz/
benchmark evidence rather than raised by convenience, are: one file and 1 MiB
decoded source; 2 MiB for the complete encoded request including framing and
terminator; one normalized repo-relative path of at most 8 KiB; exact 64-hex
content/artifact hashes; at most 32 metadata fields, each at most 256 bytes and
8 KiB in aggregate; 250,000 syntax nodes; AST depth 512; 4,096 RepoGrammar
diagnostics; 10,000 facts/unknowns; 8 MiB complete encoded framed response/
stdout; 64 KiB stderr; five wall-clock seconds; four CPU-seconds; two threads
including main; 256 MiB address space; and no descendants. Encoded request
limits are checked before spawn and decoded limits before allocation or parse.

The separate project-model parser accepts at most three selected documents per
project root (`composer.json`, optional same-root `composer.lock`, and at most
one PHPUnit XML file by precedence), each at most 1 MiB and 3 MiB decoded in
aggregate, within a 5 MiB complete encoded request. Its paths use the same
8 KiB cap; JSON/XML depth is at most 128; aggregate nodes/entries are at most
100,000; each emitted scalar is at most 2 KiB; and the complete allowlisted
normalized summary is at most 64 KiB. Every count is inclusive and requires
exact-limit, limit-plus-one, maximum-decoded-plus-envelope, and independently
oversized-encoded tests. Exhaustion is `php_resource_protocol`, never
`no_family`.

Qualification must compile and test the exact candidate on:

- `x86_64-unknown-linux-gnu`;
- `aarch64-unknown-linux-gnu`;
- `x86_64-apple-darwin`;
- `aarch64-apple-darwin`; and
- `x86_64-pc-windows-msvc`.

Linux, macOS, and Windows require native malformed-input, depth/node/output,
timeout/OOM, protocol, no-execution, filesystem/network/descendant denial, and
source-free diagnostics tests. Cross-compilation or upstream binary existence
alone is insufficient.

### D8. Atomic delivery and stop conditions

PHP work is delivered as ten independently coherent implementation/review
stages:

1. this decision and incomplete evidence/review ledger;
2. discovery/configuration inventory and exclusions;
3. frontend artifact, differential corpus, supply-chain, and sandbox
   qualification;
4. worker/protocol/artifact admission;
5. bounded Composer/PHPUnit project-profile and two-phase exclusion model;
6. PHP code units/IR plus exact namespace/import/attribute resolver;
7. typed PHP `UNKNOWN` registry and authoritative claim-impact classifier;
8. `php.phpunit.test_method`, support gate, and fixtures;
9. source-free CLI/MCP/readiness wiring;
10. independent cross-module four-part review and any scoped atomic fix commits.

Stages 1 and 2 are now complete. Stage 2 implements the exact D4 token,
precedence, exclusion, bounded raw-byte inventory, source-store/parser bypass,
warning, generation-mode, incremental, legacy-claim purge, CLI, and autosync
contracts with tests and synchronized documentation. It does not implement the
later project-model parser or custom `vendor-dir`/PHPUnit cache-directory
reclassification: exact global `vendor`, PHP-only `.composer`, and PHP-only
`.phpunit.cache` are the only active exclusion rules in this stage. Stages 3
through 10 and the separate completion audit remain open.

After stage 10, a separate final completion-audit commit links every
prerequisite and fix SHA. That audit is a terminal gate, not an eleventh
implementation stage, and must not be folded into the review/fix commit.

Each completed behavior-bearing stage requires tests. Every stage requires
documentation, an independent correctness, security, design/completeness, and
performance/resource review, full applicable gates, and an atomic Conventional
Commit. PHP remains unsupported until every ADR-0020 row is checked and the
audit links every prerequisite SHA.

Stop rather than weaken the contract if an artifact cannot be reproduced and
audited; official differential conformance fails; partial ASTs can support
anchors; target code/config/tooling can execute; any platform lacks enforceable
containment; ranges/diagnostics are nondeterministic or leak data; resource
limits are bypassable; or support, freshness, compatibility, authoritative
`UNKNOWN`, and source-free product gates cannot be demonstrated.

## Consequences

This decision selects a conservative qualification direction without claiming
that Mago, PHP-Parser, PHP CLI, Tree-sitter PHP, Composer, or PHPUnit has been
integrated. PHP now has bounded discovery-only inventory, but no semantic
capability. The decision narrows the first future PHP value slice to auditable
direct PHPUnit declarations and explicitly closes Laravel runtime routes,
indirect ancestry, provider/dependency behavior, dynamic loading, generated
proxies, and runtime execution for N1.

Primary research authorities include the official [PHP 8.5.8 release
metadata](https://www.php.net/releases/index.php?json&version=8.5.8), [PHP
release and checksum page](https://www.php.net/downloads.php?source=Y), [Mago 1.43.0
release](https://github.com/carthage-software/mago/releases/tag/1.43.0),
[`mago-syntax` 1.43.0 crate](https://crates.io/crates/mago-syntax/1.43.0),
[PHP-Parser 5.8.0 source](https://github.com/nikic/PHP-Parser/tree/v5.8.0),
[PHP-Parser 5.8.0 Packagist metadata](https://packagist.org/packages/nikic/php-parser#v5.8.0),
[Tree-sitter PHP 0.24.2 release](https://github.com/tree-sitter/tree-sitter-php/releases/tag/v0.24.2),
[`tree-sitter-php` 0.24.2 crate](https://crates.io/crates/tree-sitter-php/0.24.2),
[Composer 2.10.2 release](https://github.com/composer/composer/releases/tag/2.10.2),
[Composer 2.10.2 lock-content-hash source](https://github.com/composer/composer/blob/2.10.2/src/Composer/Package/Locker.php),
[Composer schema](https://getcomposer.org/doc/04-schema.md), [Composer untrusted
package guidance](https://getcomposer.org/doc/faqs/how-to-install-untrusted-packages-safely.md),
[PHPUnit 13.2.4 release](https://github.com/sebastianbergmann/phpunit/releases/tag/13.2.4),
[PHPUnit 13.2.4 `Test` attribute source](https://github.com/sebastianbergmann/phpunit/blob/13.2.4/src/Framework/Attributes/Test.php),
[PHPUnit 13.2.4 package manifest](https://github.com/sebastianbergmann/phpunit/blob/13.2.4/composer.json),
[PHPUnit 13.2 test rules](https://docs.phpunit.de/en/13.2/writing-tests-for-phpunit.html),
and [PHPUnit 13.2 attributes](https://docs.phpunit.de/en/13.2/attributes.html).
