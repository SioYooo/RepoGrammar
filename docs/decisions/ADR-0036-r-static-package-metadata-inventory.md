# ADR-0036: R static package metadata inventory

- Status: Accepted
- Date: 2026-08-01
- Scope: R Round-4 discovery and conservative dependency inventory under ADR-0020
- Refines: ADR-0020 and ADR-0030
- Related: `docs/reports/language-support/r-completion-review.md`

## Context

R package metadata has several evidence levels. The R Core "Writing R
Extensions" manual defines `DESCRIPTION` as Debian-control-file-style metadata,
lists `Depends`, `Imports`, `LinkingTo`, `Suggests`, and `Enhances` dependency
fields, and defines literal `import`/`importFrom` directives in `NAMESPACE`.
Those files prove declared package names and optional requirements, but not
whether each named package is from CRAN, Bioconductor, a custom repository, or
a remote/local source.

The renv lockfile documentation defines package records with package identity,
version, and installation source. Its source documentation gives
`Source: Repository` plus `Repository: CRAN` as CRAN evidence, treats
Bioconductor as a separate source, documents unknown/custom/remote/local
sources, and notes that lockfile records do not by themselves prove current
installation or use.

Primary sources reviewed on 2026-08-01:

- R Core, "Writing R Extensions":
  <https://stat.ethz.ch/R-manual/R-devel/doc/manual/R-exts.html>;
- renv, "Anatomy of a Lockfile":
  <https://rstudio.github.io/renv/articles/lockfile.html>;
- renv, "Package sources":
  <https://rstudio.github.io/renv/articles/package-sources.html>.

## Decision

R remains `discovered_only` and unsupported. Discovery admits exact `.R` and
exact lowercase `.r` source plus exact root/nested basenames `DESCRIPTION`,
`NAMESPACE`, and `renv.lock`. Both source spellings are source, not metadata:
neither is decoded, parsed, or routed to the `r-config` adapter, and they are
inventory-only. Managed `renv/library`, `renv/cache`, `renv/staging`,
`.Rproj.user`, and packrat library candidates are R-specific exclusions applied
identically to both source spellings; they do not globally prune unrelated
languages. `.Rprofile`, `.Renviron`, `.Rproj`, `.Rmd`, renv activation code,
and executable project/package selectors are not R metadata inputs.

The three admitted metadata files enter one bounded, pure Rust, non-executing
`r-config` adapter:

- `DESCRIPTION` parses bounded DCF fields and validates the five official
  dependency fields. Declarations have direct relationship evidence, but no
  dependency record is emitted because registry identity is unresolved.
- `NAMESPACE` recognizes only one-line literal unquoted `import` and
  `importFrom` package names. It does not process R-like conditionals or other
  directives. Import identity is direct but registry and version remain
  unresolved, so no dependency record is emitted.
- `renv.lock` passes the shared duplicate-key/depth/member/key-size JSON gate.
  Only records with matching bounded `Packages` key/`Package`, a bounded
  version, and exact explicit source evidence are admitted: `Source:
  Repository` plus `Repository: CRAN` becomes ecosystem `cran`; `Source:
  Bioconductor` becomes ecosystem `bioconductor`. Their version is
  `lockfile_resolved`, scope and directness are `unknown`, and installation,
  restore success, runtime selection, root-directness, and manifest coherence
  are not claimed.

Malformed, duplicate, partial, conflicting, ambiguous-ecosystem, unknown-source,
and resource-limit cases emit claim-scoped `r_dependency_inventory` typed
`UNKNOWN` facts. Custom repository, remote, URL, GitHub/GitLab/Bitbucket, local
path, and other unproved source records are omitted. Their raw values are never
copied into facts, diagnostics, dependency rows, CLI output, or storage.

Limits are inclusive and tested at exact/+1 boundaries: 1 MiB per config,
16,384 logical lines, 512 DESCRIPTION fields, 2,000 dependency/import/package
records, JSON depth 128, 8,192 object members, and 256 decoded key bytes.

RepoGrammar must never invoke R, `parse`, `eval`, `source`, profiles, renv,
package installation/restoration, native code, tests, repository scripts,
child processes, or network access. No production dependency is added. No R
source code unit, IR, framework role, family, support, or readiness record is
authorized.

## Consequences

- R-source-only generations are `file_manifest_only`; a generation with an
  admitted metadata file is `syntax_only_code_units` because it owns a bounded
  project-config unit.
- Unchanged evidence-bound lock rows copy forward; modified locks replace them;
  removed locks cannot leave stale dependency rows.
- The shared directness type remains three-state. DESCRIPTION/NAMESPACE prove
  direct declarations only at the classified-candidate level; renv rows retain
  `unknown`; no source permits a transitive assertion.
- A future R frontend, selected project model, or family requires a separate
  sandboxed and source-backed ADR-0020 stage.

## Rejected alternatives

- Treating every DESCRIPTION/NAMESPACE package as CRAN: rejected because R Core
  metadata does not prove repository source and Bioconductor/custom sources use
  the same names.
- Looking up packages online: rejected because it is mutable, network-dependent,
  and cannot prove the repository's intended source.
- Running R or renv to normalize metadata: rejected because it executes runtime,
  profile, package, and potentially native/project behavior.
