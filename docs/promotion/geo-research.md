# Search and answer-engine discovery foundation

Verified: 2026-09-30. Product baseline: GitHub `0.5.0`; this work is local and
has not been published. GEO here means making accurate technical material easy
to discover, interpret, cite, and verify. It is not a ranking promise.
The [launch kit](launch-kit.md) owns positioning and publication copy;
specifications, accepted ADRs, and release evidence remain the product authorities.

## Official guidance checked

These primary pages were actually fetched on 2026-09-30. Their guidance is
external evidence, not a RepoGrammar visibility measurement.

| Primary source | Finding relevant to this project |
| --- | --- |
| [OpenAI crawler documentation](https://developers.openai.com/api/docs/bots) | `OAI-SearchBot` controls automatic ChatGPT search crawling; `GPTBot` controls potential training use independently. `ChatGPT-User` is user-triggered access, not the search crawler. A future site can allow search while declining training. Allowing access does not prove a citation. |
| [Google generative AI optimization guide](https://developers.google.com/search/docs/fundamentals/ai-optimization-guide) | Ordinary search quality and technical requirements still apply. Useful original content, readable structure, crawlability, and reduced duplication matter. No special schema is required; Google ignores `llms.txt` for visibility/ranking. Eligibility does not guarantee crawling, indexing, or serving. The current guide describes a Generative AI performance report in Search Console; access requires verified-site evidence. |
| [Google Search generative AI control](https://support.google.com/webmasters/answer/16908024) | Check effective inclusion before deploying. Inclusion is the default, while child properties may inherit exclusion; do not invent a new opt-in registration requirement. |
| [Google spam policies](https://developers.google.com/search/docs/essentials/spam-policies#scaled-content) | Creating many low-value pages to manipulate rankings or generated answers is scaled content abuse regardless of production method. This project will not create query-variant pages, fabricated reviews, hidden keywords, or repeated syndicated drafts. |
| [Bing sitemap guidance](https://blogs.bing.com/webmaster/2025/7/Keeping-Content-Discoverable-with-Sitemaps-in-AI-Powered-Search/) | Sitemaps describe actual site URLs. Accurate `lastmod` reflects content changes, not build time. Notification and sitemap processing do not guarantee AI-answer inclusion. |
| [IndexNow FAQ](https://www.indexnow.org/faq) | Notifies participating engines about changed URLs; each decides indexing. A successful submission is receipt evidence, not indexing proof. Site/key ownership is necessary; no submission is appropriate for GitHub URLs we do not serve. |
| [Bing AI Performance public-preview announcement](https://blogs.bing.com/webmaster/2026/2/Introducing-AI-Performance-in-Bing-Webmaster-Tools-Public-Preview/) | Describes citation totals, cited pages, and sampled grounding queries in verified Webmaster Tools. Aggregated citations do not establish ranking or placement. This environment has no verified dashboard export for RepoGrammar. |

The Google guide explicitly addresses both the no-special-markup and `llms.txt`
hypotheses. No third-party ranking theory is adopted. Clear attribution and
source provenance are editorial requirements for our material; we do not
assert a special authorship ranking multiplier.

## Controlled surface and canonical answer map

The checkout has GitHub-rendered Markdown and release assets, but no deployable
documentation website was found: no Pages deployment, CNAME, site framework,
static HTML entrypoint, sitemap, or site crawler policy. `package.json` points
its homepage to GitHub. Framework fixtures and code adapters are not a site.
This is a repository-infrastructure finding, not a claim that no external
domain exists anywhere. GitHub controls `github.com` HTTP/crawler policy.

We therefore add no root `robots.txt`, sitemap, JSON-LD, or `llms.txt`, submit
no URLs, and introduce no website dependency. [The site proposal](geo-site-proposal.md)
is a separate maintainer decision.

| Question cluster | Canonical answer surface | Verifiable depth |
| --- | --- | --- |
| What it is, who it helps, how it works, alternatives | [README](../../README.md#how-it-works) | [product specification](../specifications/product.md), [optional graph boundary](../decisions/ADR-0010-optional-codegraph-provider.md) |
| UNKNOWN, partial context, bounded reads | [README](../../README.md#what-do-unknown-and-a-bounded-read-plan-mean) | [MCP contract](../specifications/mcp-api.md), [UNKNOWN contract](../specifications/unknowns.md) |
| Local analysis, agent privacy, telemetry | [README](../../README.md#does-repository-code-leave-my-machine) | [telemetry contract](../specifications/telemetry.md) |
| Installation and lifecycle commands | [README](../../README.md#quick-start) | [quickstart](../quickstart.md), [CLI contract](../specifications/cli.md) |
| Codex and Claude Code integration | [Codex guide](../quickstart-codex.md), [Claude guide](../quickstart-claude.md) | [installation contract](../specifications/installation.md) |
| Language boundary, limitations, when to use another method | [limitations](../limitations.md) | [Top-20 audit](../reports/language-support/top-20-final-program-audit.md) |
| Concrete read-plan example and counterexamples | [FastAPI/pytest walkthrough](../examples/python-fastapi-pytest.md) | Existing fixture paths, reproducible commands, expected shapes, and negative controls |
| Current GitHub versus historical npm availability | [launch-kit publication table](launch-kit.md#current-publication-truth) | [immutable v0.5.0 release summary](../release/stable-v0.5.0-release.summary.json) |

This map links existing high-information documents. It does not create a thin
page per query. Maintainer-authored source/specification history and explicit
evidence links provide provenance; no invented social proof is added.

## Baseline and corpus contract

[geo-query-corpus.json](geo-query-corpus.json) freezes 24 branded/unbranded
queries spanning problem/category discovery, comparison, implementation,
privacy, integrations, conventions, and limitations. Freeze query ids/text and
expected canonical paths before any later engine run; a changed corpus gets a
new schema/protocol revision and must not be silently compared as the same sample.

Schema v1 has `schema_version`, `baseline_date`, `repository_version`,
`protocol_ref`, and `queries`. Every query has:

- `query_id`, `query`, `intent`, `branded` boolean, and `target_concept`;
- `expected_canonical_source` with repo-relative `path`, GitHub heading `anchor`
  (empty if none), and matching public `url`;
- `observations`: engine, date, status, mention/citation booleans or null,
  cited URL, observable rank, `evidence_ref`, notes, and `unknown_reason`.

Status is `NOT_MEASURED`, `UNKNOWN`, or `OBSERVED`. Unmeasured/unknown rows keep
mention, citation, URL, rank, and evidence null with a nonempty reason. Null is
not false or zero. Observed rows require a retained capture reference and
boolean mention/citation; absent mention/citation is a measured false only
after inspecting the actual completed result. Cited URL is required for true
citation, otherwise null. Rank is an integer only for a labeled visible ordered
result list; answer citations have no inferred rank.

The 2026-09-30 baseline is **NOT_MEASURED** for all 72 query-engine pairs
(ChatGPT Search, Google Search, Bing Search). No engine-specific result captures
were collected. The research browser retrieves official guidance; those fetches
do not establish how any of those engines answers the corpus. No authenticated
Search Console, Bing Webmaster Tools, referral analytics, or crawl-log data
is available to this sprint. Indexed canonical URLs, citation counts,
description correctness, version staleness, and referrals remain unknown;
they are not zero.

## Future comparison protocol

1. Record the reviewed publication commit/version, live canonical URLs, access
   date, engine/product mode, signed-in state, model if exposed, locale/region,
   and fresh-session settings. Confirm publication first; local changes cannot
   cause discovery of URLs that have not been served.
2. Run the frozen queries independently without preceding brand hints, custom
   instructions, or site filters. Keep branded and unbranded strata separate.
   Use ordinary permitted engine access; no automation bypass, bulk abuse, or
   paid calls without authorization. Record errors/restrictions as `UNKNOWN`.
3. Retain the visible result/answer, citation URLs, timestamp, and settings in
   a sanitized capture bundle. Redact account identifiers and credentials.
   `evidence_ref` points to that retained bundle; do not commit private sessions.
4. Have a reviewer check each observed mention against the product/release
   authorities frozen for that capture. Log specific unsupported or stale
   statements and whether citations point to canonical sources or historical
   documents. Historical npm 0.4.3 is not false; calling it current GitHub
   stable is stale.
5. Repeat the same protocol at three declared timepoints over at least two
   weeks after publication. Preserve failed/unqueryable rows and all timepoints.
   Report counts and denominators by engine, intent, and branded status; small
   before/after changes are descriptive observations, not causal improvement.

| Metric | Transparent definition | Current evidence |
| --- | --- | --- |
| Mention/citation retrieval | True rows / successfully observed query-engine rows, split branded/unbranded | NOT_MEASURED |
| Correct-description rate | Mentioned answers with every reviewed product statement supported / mentioned answers reviewed | NOT_MEASURED |
| Unsupported-claim rate | Reviewed mentioned answers with at least one unsupported claim / mentioned answers reviewed | NOT_MEASURED |
| Version-staleness rate | Reviewed mentioned answers that assert an obsolete current release/install channel / mentioned answers with version assertions | NOT_MEASURED |
| Canonical-source citation rate | Canonical RepoGrammar citation URLs / all RepoGrammar citation URLs captured | NOT_MEASURED |
| Indexed URL count / crawl access | Verified owner index report / host response and crawler-log evidence, respectively | UNKNOWN; no controlled site/dashboard |
| Referral traffic | First-party analytics count with window and attribution method | UNKNOWN; no analytics |

Do not combine these into an unexplained visibility score. A submission
receipt, crawl, index entry, retrieval, citation, correct description, and
conversion are separate observations. The official dashboard metrics above
can supplement captures when access exists, but must not be conflated with
this frozen query sample.

## Maintenance

The maintainer owns launch approval and deployment. Documentation changes that
alter product facts must update the launch kit and answer surfaces in the same
coherent change under [documentation policy](../development/documentation-policy.md).
Recheck official guidance before deployment and quarterly thereafter. Corpus
validation checks shape/provenance only; it cannot prove search visibility.

Run `cargo run --quiet --bin repo-guard -- check-geo` to check the corpus,
canonical source references, explicit current-publication markers, and README
installation pins against the recorded release. This local guard does not
query engines, authenticate dashboards, refresh registry state, or validate
the authenticity of a declared result capture; those remain review obligations.

## Local validation result

The [foundation summary](geo-foundation.summary.json) records this prepared
snapshot. `check-geo` passed, its five regression tests passed, and an
independent reviewer passed 15 temporary-fixture mutations, including stale
pins, fabricated observations, missing captures, duplicate queries/engines,
URL mismatches, path escapes, nonimmutable records, and historical controls.
Formatting, Clippy, the full Rust gate, repository guard, relative links,
corpus heading anchors, and mirrored-guide equality passed. Initial guard
compile/lint errors were corrected without suppressions or weaker assertions.

These results validate local asset consistency and the regression checks.
All 72 engine observations remain `NOT_MEASURED`; no post, deployment,
indexing notification, or repository metadata change was performed.
