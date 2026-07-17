# Stable v0.2.1 Failed Candidate Record

This historical path is retained so links from the `v0.2.1` source snapshot do
not break. It is not an active publication checklist and must not be executed.

The retained annotated `v0.2.1` tag points to
`22956a2d5dc8ef19241ae86cefbe42c6709b05a5`. Its tag workflow run
`29582156611`, attempt 1, completed all artifact and private-draft gates. The
expected 11-asset private GitHub draft, id `355686885`, remained unpublished.
npm staging then failed before registry staging because the bare
`npm-candidate/sioyooo-repogrammar-0.2.1.tgz` argument was parsed as GitHub
shorthand rather than as a local package file. The npm stage inventory remained
empty. No public GitHub `v0.2.1` Release and no npm
`@sioyooo/repogrammar@0.2.1` package were published.

The tag and private draft are retained for auditability. They must not be
moved, replaced, published as another version, or reused as publication
authority. The staging command requires an explicit local package spec beginning
with `./`; the active patch-forward authority is the
[stable v0.2.2 release checklist](stable-v0.2.2-release-checklist.md).
