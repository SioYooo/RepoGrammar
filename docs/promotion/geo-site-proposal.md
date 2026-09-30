# Minimal documentation-site proposal

Status: PROPOSED; no deployment authorized or performed. Reviewed 2026-09-30.
The checkout has no standalone site infrastructure. Repository-native answers
and the [GEO corpus](geo-query-corpus.json) come first.

## Why a site may be useful

GitHub Markdown is useful and public, but the project cannot control
`github.com` crawler rules, canonical metadata, HTTP redirects, or verified
site analytics. A small owned documentation host would make those controls and
stable human-readable URLs possible. This is a control/maintenance rationale,
not evidence that GitHub content is poorly indexed or that a site will rank
better. Visibility is currently not measured.

Recommendation: approve a minimal static publication only if a maintainer will
own the domain/deployment and keep it current. Do not launch a separate content
factory or rewrite specifications into another authority.

## Proposed information architecture

Choose the actual HTTPS origin before implementation; none is invented here.

| Intent / path | Title | Description and source |
| --- | --- | --- |
| Identity `/` | RepoGrammar: implementation-family context for coding agents | Local evidence, bounded reads and UNKNOWN; README identity/how-it-works sections |
| Lifecycle `/docs/quickstart/` | Install and initialize RepoGrammar | GitHub binary install, init/sync/resync/status/doctor distinctions; quickstart and CLI specification |
| Codex `/docs/agents/codex/` | Use RepoGrammar with Codex | Exact wiring, source/public distinction and conditional use; Codex quickstart |
| Claude `/docs/agents/claude-code/` | Use RepoGrammar with Claude Code | Exact wiring, read plan and fallback; Claude quickstart |
| Evidence `/docs/evidence/` | A source-backed FastAPI and pytest walkthrough | Reproducible fixture, positive and negative cases; existing example |
| Limitations `/docs/limitations/` | RepoGrammar evidence and language limits | Bounded scope, UNKNOWN, static alignment and when to use other tools; limitations |
| Publication `/docs/releases/` | RepoGrammar publication channels | Current GitHub versus separately dated npm channel; launch-kit publication projection and immutable release records |

Use stable paths, one canonical page per topic, and source/edit links to the
exact reviewed Git commit. Render existing Markdown through one explicit map;
do not maintain two prose copies or serve experiments, raw traces, private
content, and unreviewed generated pages. Any future broader documentation
export should preserve current authority and avoid duplicate answer pages.

## Technology, ownership, and cost

Prefer static GitHub Pages output through its supported Markdown/Jekyll path,
subject to repository-guard and supply-chain review. No JavaScript application
framework or product-runtime dependency is needed. All repository-specific
build/validation logic must live under `src/` and reuse `repo-guard`; deployment
workflow YAML only invokes it. If the hosting path needs new pinned build
dependencies/actions, review and approve them separately before implementation.

Maintainer: name a real deployment/content owner and approve repository Pages
settings, HTTPS origin, and release policy. Contributor reviews cover product
facts and links; only the owner publishes. Static hosting can use an existing
free allowance, but no billing plan has been checked; a custom domain has
registration/renewal cost. Budget one small release-time review and quarterly
crawler/link/access review. No credentials or external account changes occur
in this sprint.

## Discovery and privacy contract for a future deployment

- Critical text must appear in static HTTP HTML without mandatory JavaScript,
  return 200, and remain accessible on mobile and assistive technology.
- Set unique title/description, self-canonical HTTPS URL, source link, and
  truthful OpenGraph metadata. Preserve old URLs with explicit redirects if
  paths change; audit for accidental `noindex`.
- Serve `robots.txt` at the controlled origin root. Recommend allowing
  `OAI-SearchBot` for intended search discovery, while the maintainer chooses
  `GPTBot` training policy independently. Avoid disguising user-triggered
  `ChatGPT-User` access as automatic search crawling. Test actual host paths and
  bot access, not repository file presence.
- Publish an XML sitemap containing only served canonical pages, with
  `lastmod` from actual content changes. Check URLs/status/canonical equality
  against the deployed path and add regression checks.
- If useful for rich results, add truthful schema.org `SoftwareSourceCode`
  metadata for the repository and `TechArticle` for technical documentation.
  Identify the actual maintainer and license; no fabricated ratings or aggregate
  usage. Validate rendered structured data before publication. No special AI
  schema is required.
- `llms.txt` is optional only for a demonstrated consuming system and kept in
  sync with canonical pages. It is not a Google visibility/ranking mechanism.
- Owner verifies the actual host in Google Search Console and Bing Webmaster
  Tools using approved DNS/HTML verification. At deployment, recheck the
  current Google guide and the [effective Search Console control](https://support.google.com/webmasters/answer/16908024)
  for generative AI features, including inherited parent settings. Inclusion is
  the documented default, so do not invent a mandatory opt-in registration.
  Record ownership/access, effective inclusion, and index reports separately.
  IndexNow is optional after key/host verification and only
  for changed URLs; notifications never count as indexing or citation proof.
- Start without visitor tracking scripts. If needed, prefer owner dashboard
  aggregates or privacy-preserving first-party metrics with a documented
  retention policy; never log repository source or agent prompts. Reuse the
  [measurement protocol](geo-research.md#future-comparison-protocol).

These choices derive from the dated [official-guidance research](geo-research.md#official-guidance-checked).
Deployment acceptance requires link/canonical/robots/sitemap/metadata checks,
signed-out live access, and an explicit owner publication approval. Local build
success is not a deployed-site or indexing result.

## Exact decision requested for a follow-up

Approve or decline a static GitHub Pages docs host; if approved, name the
deployment owner, choose the HTTPS origin/custom-domain budget, choose the
independent search/training crawler policies, and authorize the required
repository settings and deployment. Until then, retain GitHub-native material
and collect a truthful query baseline. This proposal itself grants none of
those permissions.

## Implementation handoff after d981d57

This is the implementation-ready design for a future static publication, not
authorization to configure Pages, DNS, owner accounts or indexing submissions.
The origin, deployment owner and independent GPTBot policy remain decisions.
Keep one rendered canonical page per intent and retain source documents as
authority. No website framework or dependency is needed for this preparation.

Descriptions above are proposed metadata, not new independent product prose.
Critical answers, commands and limitations must render as static HTML. Do not
split these into query-variant landing pages. Do not export the full README to
both `/` and `/docs/quickstart/`: map identity sections to `/` and lifecycle
content to quickstart. Every page has one absolute self-canonical URL under
the approved origin, one h1, unique title/description, accessible navigation,
source/edit links pinned to the publication commit and truthful OpenGraph text.
Link to the original specifications rather than duplicating them. GitHub copies
cannot be assigned custom canonical metadata by this repository.

Build a single explicit page/source map under `src/` using the approved static
Markdown renderer. Record all content dependencies per page, including shared
release/scope projections. Use rendered-content hashes to distinguish content
changes from rebuilds; `lastmod` is the last actual content change timestamp
from reviewed history, never the CI build time. If that provenance is unknown,
omit optional lastmod rather than invent it. The sitemap includes only served
canonical 200 URLs. Do not add `priority` or `changefreq` as a ranking tactic.

Render `robots.txt` at the controlled origin root only. The owner chooses
GPTBot training access independently from intended OAI-SearchBot search
access; permitting the crawler is not indexing/citation proof. Account for
host/CDN blocks and actual bot IP access as well as robot rules. Optional
`SoftwareSourceCode` describes the real repository, maintainer and MIT license;
`TechArticle` applies only to actual technical pages with truthful author/source
and modification dates. No ratings, fabricated usage or special AI schema.
Optional llms.txt may index those same canonical pages for a demonstrated
consumer; Google ignores it for ranking/visibility.

Before publishing, run deterministic page-map uniqueness/internal-link tests,
HTML title/description/canonical and content-with-JavaScript-disabled checks,
schema/robots/sitemap validation, historical-version controls and
source/public-install consistency. Mutant tests must reject duplicate canonical
URLs, future/fabricated lastmod, stale current npm/GitHub assertions, unintended
noindex and bot-policy conflation. After separate owner approval, verify HTTP
status/content/redirects and metadata signed out; local build PASS is not live
accessibility PASS.

Google Search Console: the owner selects Domain verification through approved
DNS or a URL-prefix property through a retained verification HTML file/tag.
Bing Webmaster Tools: owner verifies the same origin by approved DNS/XML/meta
verification or authorized Search Console import. Retain verification tokens
as deployment secrets/config, never in research results. Check effective
generative-search inclusion, including inherited settings; no account change
or submission is performed here. Sitemap receipt, index report and actual
retrieval are separate evidence. Optional IndexNow requires host/key ownership
and only truthful changed URLs; it does not guarantee indexing.

Execute the existing 24-query/72-cell [baseline protocol](geo-research.md#future-comparison-protocol)
**before publication** when permitted access exists, then at the three declared
timepoints. Freeze the corpus hash and capture settings. Do not replace its
GitHub expected URLs with site URLs until a versioned URL-map revision is
publicly served; preserve the old baseline. Canonical-claim accuracy is the
fraction of reviewed mentioned answers whose product/version/platform/privacy/
measurement statements are all supported by frozen authorities. Report its
numerator, reviewed denominator and specific unsupported/stale statements.
Absent captures remain NOT_MEASURED, never zero. Index counts require verified
dashboard exports; ranking, citation lift and causal GEO effects are unproved.
