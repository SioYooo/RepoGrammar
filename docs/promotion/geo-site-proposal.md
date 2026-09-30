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

| Proposed path | Content authority |
| --- | --- |
| `/` | README product description, install entrypoint, release and scope boundary |
| `/docs/quickstart/` | Existing quickstart and lifecycle commands |
| `/docs/agents/codex/`, `/docs/agents/claude-code/` | Existing agent guides |
| `/docs/evidence/` | Existing fixture walkthrough and links to immutable specification evidence |
| `/docs/limitations/` | Existing limitations and language-audit links |
| `/docs/releases/` | Current publication projection with explicitly dated historical release links |

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
