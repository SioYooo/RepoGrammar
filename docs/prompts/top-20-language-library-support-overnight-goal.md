# Top-20 Language and Third-Party Library Support Overnight Goal

```text
/goal

在仓库 `/Users/sioyoo/code/RepoGrammar` 中，执行一次“Top-20 编程语言完整支持 + 可扩展第三方库语义分析平台”的重大功能开发。

这是一次夜间无人值守任务。你应当尽可能自主、完整地推进全部要求，但必须保持证据保守、变更隔离、结果可审计。不要因为工作量大就只写计划或停在调研阶段；应持续实现、测试、审查、提交，直到满足成功标准、遇到无法安全消除的真实阻塞，或完成最多 5 个自主轮次。

最重要的 Git 限制：

- 绝对不要把本次修改 merge、rebase、squash、cherry-pick 或以任何方式放入 `main`。
- 不要修改 `main` 分支引用。
- 不要 push 任何分支、tag、commit 或 release。
- 不要创建 PR。
- 不要执行 `git pull`、远程 merge、release、publish 或部署。
- 所有工作只能保存在新的专用 feature branch、它的子分支以及隔离 worktree 中。
- 本次工作不能影响当前 RepoGrammar 参赛提交的评分基线。
- 截至 2026-07-31，已验证的评分基线为：
  - `main`: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
  - `origin/main`: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
  - 当前专用分支：`feat/top-20-language-library-support`
  - 当前 worktree 在开始本任务前为 clean
- 开始时必须重新验证以上事实。若它们发生变化，不要擅自更新、拉取或修改 `main`；记录实际状态，并从当前明确确认的评分基线建立隔离分支。
- 如果 `feat/top-20-language-library-support` 已存在、仍以该基线为祖先且没有不明改动，就继续使用它作为 integration branch。
- 如果该分支不存在，则从上述基线 SHA 创建：
  `feat/top-20-language-library-support`
- 不得从一个包含未知未提交改动的工作区创建分支。
- 如果原始 checkout 需要保持完全不动，优先在 `/private/tmp` 下建立独立 worktree 和子分支；不要覆盖或删除原 checkout。
- 子分支只能集成进 `feat/top-20-language-library-support`，永远不能集成进 `main`。
- 最终报告必须提供任务开始和结束时的 `main`、`origin/main` SHA，证明评分基线没有改变。

# 1. Mission Contract

## 1.1 主要目标

尽可能完整地实现 RepoGrammar 冻结 Top-20 语言计划，同时建立一个能够覆盖任意第三方依赖的通用分析架构。

冻结 Top-20 范围为：

1. Python
2. C
3. C++
4. Java
5. C#
6. JavaScript
7. Visual Basic
8. SQL
9. R
10. Rust
11. Delphi/Object Pascal
12. Scratch
13. Go
14. PHP
15. Swift
16. Ada
17. Assembly
18. MATLAB
19. Fortran
20. Ruby

TypeScript 是额外支持语言，必须单独完成和汇报，但不能计入 Top-20 的 20 个名额。

不得自行替换这份语言清单，也不得使用其他实时语言排行榜改变范围，除非仓库中已有更新 ADR 明确取代当前冻结范围。

## 1.2 第三方库支持的正确含义

“支持所有第三方库”不能被实现成一份有限的硬编码库名单，也不能声称理解任意库的全部运行时行为。

本任务应实现两层能力：

第一层：对任意第三方包的通用分析能力。

对于已经完成的每一种语言，系统至少应能够在不执行项目代码的前提下：

- 发现其主要 manifest、lockfile、workspace、module、package、build metadata。
- 建立规范化的 package identity。
- 记录 package name、ecosystem、version、source、checksum或摘要、workspace关系。
- 区分标准库、本地 workspace 包、直接依赖、传递依赖、生成依赖和无法解析依赖。
- 解析 import、include、use、require、namespace、module 或等价关系。
- 在权威 provider 可用时解析第三方符号、类型、调用、继承、实现、注解、属性、decorator 或等价事实。
- 对外部符号给出稳定的 package/module/symbol identity。
- 记录每个事实的 provider、版本、输入摘要、证据来源、freshness 和置信状态。
- 当解析不足时返回 typed `UNKNOWN`，不得猜测。
- 对没有专用库适配器的依赖，仍提供通用 package、module、symbol 和重复用法分析。
- 不得把普通 import 成功夸大成已经理解框架的路由、事务、依赖注入或生命周期语义。

第二层：有版本边界的 Library Contract / Adapter Pack。

对于高价值框架和库，可以增加显式、可测试、可审查的语义合同，包括：

- Web route 或 handler 注册。
- 测试发现和测试 fixture。
- ORM model、query 和 transaction boundary。
- Validation、schema 和 serialization。
- Dependency injection。
- CLI command 注册。
- Async task、actor、job 或 event lifecycle。
- Framework-specific inheritance、annotation、attribute、decorator、macro 或 builder semantics。

每个 Library Contract 必须声明：

- ecosystem；
- package identity；
- 支持的版本窗口；
- 匹配的精确符号、annotation、attribute、decorator、macro 或 API；
- 能证明的语义；
- 明确不能证明的语义；
- alias、re-export、wrapper 和 indirect usage 策略；
- typed UNKNOWN 条件；
- positive、lookalike、negative、低支持度、parse-degraded、stale、conflict 和 resolved/unresolved fixtures；
- provider 和来源 provenance；
- source-free 输出要求；
- 安全和资源限制。

不得输出 `ALL_THIRD_PARTY_LIBRARIES_FULLY_SUPPORTED` 一类无法证伪的结论。

正确的成果标签应是：

- `LIBRARY_ANALYSIS_PLATFORM_COMPLETE`：表示通用包、依赖、外部符号和证据架构达到定义好的完成条件。
- `<ecosystem/framework>_CONTRACT_COMPLETE`：表示一个明确版本范围内的具体合同完成。
- `PARTIAL_AUDITED_PROGRESS`：表示有真实实现和证据，但没有达到全量完成标准。

## 1.3 允许的最终结果

首选成功结果：

- `TOP20_COMPLETE`
- `TYPESCRIPT_EXTRA_COMPLETE`
- `LIBRARY_ANALYSIS_PLATFORM_COMPLETE`

但这些标签只有在相应严格条件全部满足时才能使用。

允许的保守结果包括：

- `PARTIAL_AUDITED_PROGRESS`
- `QUALIFIED`
- `NO_GO`
- `BLOCKED`
- `INCONCLUSIVE`
- `ENVIRONMENT_BLOCKED`
- `DEPENDENCY_BLOCKED`
- `SANDBOX_BLOCKED`
- `PROVIDER_UNAVAILABLE`
- `LICENSE_BLOCKED`
- `DIALECT_AMBIGUITY`
- `RESOURCE_LIMIT`
- `TEST_FAILURE`
- `SECURITY_FINDING`
- `SOURCE_EVIDENCE_INSUFFICIENT`

失败、阻塞或 source-backed NO_GO 是可接受的诚实结果，但不能用它们伪装成语言完成。

所有 20 种语言和 TypeScript 都必须在最终矩阵中有明确结论。不得静默跳过某一种语言。

# 2. Fresh-Session Context

## 2.1 必读文件

在寻找实现位置或修改代码之前，完整阅读：

1. `AGENTS.md`
2. `CLAUDE.md`
3. `docs/README.md`
4. `.agents/skills/implement-change/SKILL.md`
5. `.agents/skills/major-feature-workflow/SKILL.md`
6. `.agents/skills/repogrammar-domain/SKILL.md`
7. `.agents/skills/rust-quality/SKILL.md`
8. `.agents/skills/documentation-sync/SKILL.md`

如果触及对应范围，还必须完整阅读：

- `.agents/skills/mcp-contract-change/SKILL.md`
- `.agents/skills/repogrammar-cli/SKILL.md`
- `.agents/skills/agent-integration/SKILL.md`
- `.agents/skills/telemetry-and-metrics/SKILL.md`

继续阅读：

- `docs/adr/` 下与语言、parser、semantic provider、UNKNOWN、依赖、worker、MCP 和公开支持级别相关的 ADR。
- 尤其检查 ADR 0015 至 0025 中现存文件。
- `docs/plans/v0.1-parallel-development-plan.md`
- `docs/plans/python-v0.1-implementation-plan.md`
- `docs/plans/swift-n1-qualification-handoff.md`
- 所有当前语言支持、解析器、依赖模型、semantic worker、模块边界、产品能力矩阵和测试文档。
- `.agents/memories/` 中与语言支持、第三方依赖、UNKNOWN policy、Python v0.1、CodeGraph provider 和语言扩展有关的 durable memory。

先用 `rg --files` 列出准确文件名，不要猜测不存在的路径。

## 2.2 必须重新验证的当前状态

把以下内容视为“待验证的先验快照”，不是无需检查的事实：

- 当前模型可能已经识别：
  Python、TypeScript、JavaScript、Rust、Java、C#、C、C++、Go、PHP、Ruby、Swift。
- Parser substrate 可能已有：
  Python、TypeScript、JavaScript、Rust、Java、C#、C、C++。
- Go、PHP、Swift、Ruby 可能仍是 inventory/discovery-only。
- Visual Basic、SQL、R、Delphi/Object Pascal、Scratch、Ada、Assembly、MATLAB、Fortran 可能尚未实现。
- Semantic provider registry 可能只有 TypeScript compiler 已集成，而 Python 和 Rust provider 仍是未集成 slot。
- 公开产品矩阵可能已经描述：
  - Python：FastAPI、pytest、Pydantic、SQLAlchemy；
  - TypeScript/JavaScript：Express、Jest/Vitest、Mocha/node:test、Next.js、Fastify、Prisma、Drizzle、Zod、NestJS、Hono；
  - Rust：serde、thiserror、tokio、clap、axum；
  - Java：Spring、JUnit/TestNG、JPA、JAX-RS；
  - C#：ASP.NET Core、EF Core、xUnit/NUnit/MSTest；
  - C/C++：GoogleTest、Catch2、doctest、Boost.Test。
- 当前严格的 Top-20 完成数可能仍是 `0/20`，因为语言 completion review 和九项 gate 证据不完整。
- `docs/reports/language-support/` 下的 completion review 可能缺失。
- `docs/reports/unknown-resolution-sota-analysis.md` 可能被引用但不存在。
- `repo-guard check` 可能没有捕获上述文档引用漂移。

必须从实际代码、测试、文档、生成报告和 Git 历史验证，不得把这份快照直接变成最终结论。

## 2.3 RepoGrammar 与 CodeGraph preflight

这是受 RepoGrammar pre-flight gate 覆盖的重大实现任务。

在读取非平凡源码、搜索实现模式或做架构决定之前：

1. 确认 `.repogrammar/` 是否存在。
2. 对每个实质上不同的实现 locus，构造一个精确目标：
   - 优先 repo-relative 文件或 locator；
   - 然后 `unit:`、member 或 symbol；
   - 然后明确 framework role；
   - 最后才使用简洁 pattern question。
3. 对该目标调用一次：
   - `operation: "find_analogues"`
   - `mode: "compact"`
4. 消费返回的 `read_plan`；已经返回的 line-numbered source spans 视为已读。
5. 若返回 `UNKNOWN`，最多检查一个候选 family，且只能作为候选上下文，不能当作 conformance 证明。
6. 若返回多个候选，缩窄目标，不要随意选择。
7. 若 RepoGrammar 明确返回 `FALLBACK`、stale、omitted 或 insufficient，先记录理由，然后再使用 CodeGraph。
8. 如果 `.codegraph/` 存在，在 RepoGrammar fallback 后优先使用 `codegraph explore`，再使用普通 `rg` 和源码读取。
9. 不要运行日常 `repogrammar stats`。
10. 不要静默执行 RepoGrammar initialize、resync 或 autosync。

不要用一个宽泛查询替代所有语言和所有 provider 的具体 preflight。

# 3. Hard Constraints

## 3.1 Git 与评分隔离

- 不得 merge 到 `main`。
- 不得 rebase 到 `main`。
- 不得 fast-forward `main`。
- 不得 checkout 后把 feature commit 应用到 `main`。
- 不得 push。
- 不得创建或更新 remote branch。
- 不得创建 PR、tag、release。
- 不得修改评分基线 commit。
- 不得删除尚未进入 `main` 的本次 feature branch。
- 不得使用 `git reset --hard`、强制 checkout、force push 或历史重写。
- 不得丢弃用户已有修改。
- 发现 dirty worktree 时先识别文件归属；无法安全隔离时停止该 lane，但继续其他不冲突 workstream。
- 所有 staged path 必须显式列出。
- 每次 commit 前必须检查 staged diff。
- 所有提交使用已有 maintainer Git identity；禁止添加 AI、模型、供应商或工具身份的作者、committer、co-author、signed-off-by 或 trailer。

## 3.2 架构与仓库边界

- 所有 source、executable、test、benchmark、migration tool、fixture-source、automation-tool code 必须位于 `src/`。
- 非平凡 CI 和仓库自动化逻辑必须放在 `src/rust/bin/repo_guard.rs`。
- 不得创建嵌套 `AGENTS.md`、`CLAUDE.md` 或 competing instruction files。
- 遵循 `docs/architecture/` 的模块边界和依赖方向。
- 核心 domain 只能依赖 RepoGrammar-owned types。
- 第三方 compiler/parser SDK 类型必须停留在 adapter、worker 或 anti-corruption boundary，不得泄漏进核心 domain、storage、CLI 或 MCP public schema。
- 跨路径分类决策必须只有一个 authoritative classifier/policy entrypoint。
- Tree-sitter 只能作为 syntax/candidate-generation 层，不能独自证明语义 family membership。
- 所有无法证明的静态分析事实都必须是 typed `UNKNOWN`。
- 不得把 heuristic 变成确定事实。
- 不得新增 `callers`、`callees`、`impact`、`affected`、`node` 或 `explore` 顶级 v0.1 CLI 命令。
- CLI 必须保持 pattern-family-first。
- Windows installer 和 product uninstall 仍是 deferred source-only 范围，不得把本任务扩展到 Windows release support。
- 新增 production dependency 前必须证明必要性，并同步 architecture/decision 文档。
- 优先使用现有公开 API、repo helper、平台能力或已经安装的依赖。
- 不得进行无关重构、全仓格式改写、无关依赖升级或 speculative abstraction。

## 3.3 安全约束

把以下内容全部视为不可信输入：

- repository paths；
- source files；
- archives；
- manifests；
- lockfiles；
- dependency metadata；
- compiler/provider 输出；
- subprocess stdout/stderr；
- MCP payload；
- database values；
- generated files。

Provider 和 parser 集成必须：

- 默认不执行用户项目代码。
- 不运行 package lifecycle scripts。
- 不运行 build scripts、procedural macros、annotation processors、source generators、plugins 或 arbitrary evaluators，除非另有非常明确、经过 ADR 批准的隔离合同。
- 使用明确超时。
- 限制输出大小。
- 限制递归、文件数量、archive expansion、symbol 数量和内存增长。
- 使用隔离临时目录。
- 防止 path traversal、symlink escape 和 archive bomb。
- 默认禁止 provider 在分析过程中访问网络。
- 清理临时资源。
- 不吞掉 provider crash、timeout 或 malformed output。
- 把 provider failure 映射为 typed error/UNKNOWN，而不是悄悄降级成“成功”。
- 不收集、提交或输出 secrets。
- source-free public output 不能泄露 repository source text、绝对路径或敏感 manifest 内容。

## 3.4 资源策略

默认使用：

- 本地；
- 离线；
- deterministic；
- 无付费 API；
- 无 GPU；
- 无 Slurm；
- 无外部 LLM 作为正确性 oracle。

允许读取公共官方文档和下载公开、无需认证的 provider/tool artifact，但必须：

- 优先官方 source、release、文档或 language specification；
- 不使用凭据；
- 不产生费用；
- 不做 system-wide install；
- 放入临时目录或明确允许的缓存；
- 记录 URL、版本、hash 和 license；
- 不把大型二进制或下载缓存加入 Git；
- 若网络不可用，分类为 environment/provider blocker，并继续其他 lane。

# 4. Truth Criteria and Evidence Ladder

## 4.1 每种语言的九项完成 gate

一种语言只有在以下九项全部完成时才能计入 `TOP20_COMPLETE`：

1. Discovery/config gate
   - 文件发现、扩展名、shebang 或 project metadata；
   - 忽略规则；
   - manifest/build/workspace 配置；
   - 大小、数量和安全边界。

2. Authoritative frontend/parser gate
   - 明确版本和 dialect；
   - 权威 parser/compiler/provider 或经过批准的 bounded frontend；
   - parser failure、partial parse 和 degraded mode 可观察；
   - Tree-sitter 不得作为唯一语义 oracle。

3. Owned units/IR gate
   - 核心 domain 使用 RepoGrammar-owned type；
   - source span、symbol identity、package identity、relation 和 provenance 明确；
   - provider SDK 类型被 adapter 隔离。

4. Typed UNKNOWN gate
   - unresolved import、dynamic dispatch、macro、generated code、conditional compilation、version ambiguity、dialect ambiguity 等均有明确 UNKNOWN；
   - 不允许 silent fallback；
   - stale/conflict/unavailable provider 可区分。

5. Exact-anchor family gate
   - 至少一个 language-specific、source-backed family；
   - 至少 3 个有效实例或满足仓库规定的最低支持度；
   - exact anchors；
   - lookalike 不得误判；
   - structural similarity 不能独立证明 membership。

6. Fixture/test gate
   - positive；
   - negative/lookalike；
   - low-support；
   - malformed；
   - parse-degraded；
   - timeout/resource-bound；
   - stale/conflict；
   - resolved/unresolved；
   - source-free serialization；
   - deterministic rerun。

7. Source-free readiness gate
   - CLI/MCP/storage/JSON 输出不泄露源码；
   - 输出稳定、bounded、版本化；
   - public compatibility 有测试。

8. Four-part review gate
   - correctness；
   - security；
   - completeness；
   - performance；
   - 每项有结论、证据和剩余风险。

9. Atomic delivery gate
   - implementation、tests、docs 同步；
   - completion review；
   - prerequisite commit SHA；
   - required validation 全部通过；
   - coherent Conventional Commit；
   - final completion audit。

只有达到 `bounded_preview` 或更强、并且上述九项全部满足，才能把该语言计入 Top-20 完成数。

## 4.2 语言状态枚举

最终必须给每种语言选择一个状态：

- `not_started`
- `discovered_only`
- `structural_substrate`
- `bounded_preview`
- `provider_backed`

这表示能力等级，不等同于九项 gate 是否完成。

还必须有单独字段：

- `completion_gates_passed: 0..9`
- `top20_complete: true|false`
- `provider_status`
- `library_analysis_status`
- `primary_unknowns`
- `evidence_paths`
- `prerequisite_commits`

## 4.3 证据等级

Primary evidence：

- 编译通过；
- targeted tests；
- integration tests；
- adversarial fixtures；
- deterministic rerun；
- provider version probe；
- exact output assertions；
- source-free assertions；
- full repository gates；
- committed completion review；
- prerequisite SHA 可追踪。

Auxiliary evidence：

- 官方文档；
- source inspection；
- provider help/version output；
- benchmark；
- CodeGraph/RepoGrammar analogue；
- manual diff review。

Diagnostic-only evidence：

- 单个 happy-path 示例；
- parser 产生 AST；
- package 被发现；
- import 字符串匹配；
- framework 名字出现；
- 一个 provider 在开发机上启动成功。

禁止用于强结论的证据：

- 仅 README 声明；
- 仅文档计划；
- 仅 dependency 存在；
- 仅 Tree-sitter node shape；
- 仅一条 fixture；
- 仅一个 lucky run；
- 仅无 crash；
- 仅编译成功；
- 仅 generic import；
- 未记录版本的外部工具输出；
- 运行了项目代码后获得但无法隔离的结果；
- agent 自己推断却没有 artifact 的“完成”。

# 5. Target Architecture for Third-Party Analysis

先审查当前 domain 和 storage；如果已有等价抽象，扩展现有模型，不要重复创建。

期望能力至少覆盖以下概念，名称可根据仓库现有命名调整：

- `Ecosystem`
- `PackageIdentity`
  - ecosystem
  - canonical name
  - version/version range
  - source kind
  - source locator 的安全摘要
  - content or lock digest
- `DependencyRecord`
- `DependencySnapshot`
- `ExternalSymbolId`
  - package
  - module/namespace
  - symbol
  - signature/disambiguator
- `LibraryContractId`
- `LibraryRole`
- `ProviderProvenance`
- `ResolutionStatus`
- `UnknownReason`
- `EvidenceRef`
- `FreshnessStatus`

必要的 port 可包括：

- `DependencyModelProvider`
- `LanguageSemanticProvider`
- `LibraryContractRegistry`
- `ExternalSymbolResolver`
- `ProviderCapabilityProbe`

这些只是职责要求，不是强迫创建同名 trait。若仓库已有合适模型，必须复用。

## 5.1 通用依赖能力

对于每个已完成语言，至少覆盖该生态最主要的依赖元数据：

- Python：
  `pyproject.toml`、常见 lockfile、distribution metadata、`.pyi`、`py.typed`。
- JavaScript/TypeScript：
  `package.json`、npm/yarn/pnpm lockfile、workspace、`.d.ts`、exports/imports。
- Rust：
  `Cargo.toml`、`Cargo.lock`、cargo metadata 的静态结果。
- Java：
  Maven、Gradle、classpath、JAR symbol metadata。
- C# 和 VB.NET：
  `.csproj`、`.vbproj`、MSBuild、NuGet、assembly metadata。
- C/C++：
  `compile_commands.json`、include path、target metadata，以及已有架构允许的 CMake/vcpkg/Conan 静态信息。
- Go：
  `go.mod`、`go.sum`、workspace/module metadata。
- PHP：
  `composer.json`、`composer.lock`。
- Swift：
  `Package.swift` 的安全静态模型、`Package.resolved`。
- Ruby：
  gemspec、`Gemfile`、`Gemfile.lock`、RBS metadata。
- R：
  `DESCRIPTION`、`NAMESPACE`、`renv.lock`。
- Ada：
  GPR/Alire metadata。
- Fortran：
  `fpm.toml` 和明确允许的 project metadata。
- Delphi/Object Pascal：
  `.dproj`、`.lpi`、`fpmake` 或所选工具链的静态 metadata。
- MATLAB：
  project/toolbox metadata，但不得执行 MATLAB。
- SQL：
  migration/schema/catalog 来自 repository-controlled artifacts，不连接数据库。
- Assembly：
  include、target triple、dialect、object format、build metadata。
- Scratch：
  `.sb3` 的 `project.json` 和资产索引，安全展开归档。

如果某种格式会执行代码，例如 Gradle、Package.swift、gemspec 或某些 build DSL，不得直接执行。优先采用静态、有界解析；无法安全解析的表达式保持 UNKNOWN。

## 5.2 Library Contract 行为

优先支持能够提升多个语言生态复用价值的 role：

- `web.route`
- `test.case`
- `test.fixture`
- `data.model`
- `data.query`
- `data.transaction`
- `validation.schema`
- `serialization.mapping`
- `dependency.injection`
- `cli.command`
- `async.task`
- `event.handler`

一个 contract 只能声明由 exact anchors 和 provider facts 支持的 role。禁止根据包名直接推断整个文件或符号的行为。

# 6. Language Workstreams

在正式实现前，先生成当前能力矩阵和 gap analysis。以下是目标方向，不得代替 source-backed preflight。

## 6.1 Python

目标：

- 审计并强化现有 Python-first 支持。
- 保持 FastAPI、pytest、Pydantic、SQLAlchemy 的正式合同与产品文档一致。
- 对 Django、Flask、unittest、click/typer、Celery 等 preview 能力进行真实能力审计，不得自动升级为 Supported。
- 资格审查并集成 Pyrefly provider。
- 如确有必要，使用 Pyright 做有限交叉验证；不得形成两个相互冲突的 authoritative classifier。
- 支持 `.pyi`、`py.typed`、distribution/package metadata。
- 解析 pyproject/lock/workspace，而不执行 setup、plugin 或项目代码。
- 处理 namespace package、re-export、star import、dynamic import、monkey patch 和 generated code UNKNOWN。
- 默认不得使用 RightTyper 或运行时 instrumentation 作为分析路径。
- 完成至少一个经过九项 gate 的 exact-anchor family，并回归现有 Python 产品合同。

## 6.2 C

目标：

- 将 C 与 C++ 的语言身份、dialect 和行为边界分开。
- 基于 Clang/clangd/libclang 或仓库批准的等价权威前端。
- 使用 `compile_commands.json`、include path、defines、target triple。
- 处理 header、macro、conditional compilation、platform variant。
- 选择一个真正 C-specific、source-backed 的 family；不得仅复制 C++ family 并改名字。
- 若没有充分证据选择 family，给出 source-backed NO_GO，而不是发明语义。
- 不执行编译产物。
- 对 macro expansion、missing header、variant conflict 保持 UNKNOWN。

## 6.3 C++

目标：

- 使用有界的 Clang/clangd/libclang translation-unit 语义。
- 处理 header/source、namespace、template、overload、macro、conditional compilation。
- 加强 GoogleTest、Catch2、doctest、Boost.Test 中至少一个 exact-anchor family。
- Qt 只能维持 context/UNKNOWN，除非新增明确合同和证据。
- 模板实例化、generated code、macro registration 和 compile variant 不足时不得猜测。

## 6.4 Java

目标：

- 评估 javac、Eclipse JDT 或现有批准 provider。
- 建立 Maven/Gradle/classpath/JAR symbol 模型。
- 加强 Spring MVC/Boot/Data、JUnit/TestNG、JPA、JAX-RS 中至少一个合同。
- Mockito 只能作为上下文，除非完成独立 contract。
- Lombok、annotation processing、generated sources 默认 UNKNOWN。
- 不执行 Gradle task、annotation processor 或 Spring runtime。
- 处理 overloaded methods、inheritance、interfaces、annotations 和 external symbols。

## 6.5 C#

目标：

- 使用 Roslyn Compilation/SemanticModel。
- 支持 MSBuild/NuGet/assembly metadata。
- 加强 ASP.NET Core、EF Core、xUnit/NUnit/MSTest 中至少一个合同。
- Source generator、analyzer plugin、runtime reflection 保持 UNKNOWN，除非经过单独安全资格审查。
- 不执行项目 build target 或 generator。
- 处理 attributes、extension methods、partial types、generic resolution。

## 6.6 JavaScript

目标：

- 复用 TypeScript compiler 的 `allowJs`/`checkJs` 或现有 provider。
- 支持 CommonJS、ESM、package exports、workspace、lockfile、`.d.ts`。
- 加强 Express、Jest/Vitest、Mocha/node:test、Next.js、Fastify、Prisma、Drizzle、Zod、NestJS、Hono 中至少一个合同。
- Bundler aliases、dynamic require、eval、runtime monkey patch 保持 UNKNOWN。
- React 不能因为 JSX 或 package presence 被宣称为正式支持；行为级 React 支持必须有独立 ADR/contract。

## 6.7 Rust

目标：

- 资格审查并集成 rust-analyzer、rustc 或 rustdoc JSON 的有界能力。
- 复用 cargo metadata，但不运行 build scripts、proc macros 或测试。
- 支持 `Cargo.toml`、`Cargo.lock`、workspace、feature 和 target metadata。
- 加强 serde、thiserror、tokio、clap、axum 中至少一个合同。
- 对 cfg、feature combination、proc macro、generated code、trait dispatch 保持精确 UNKNOWN。
- provider 不可用时保留 structural substrate，不得谎称 provider-backed。

## 6.8 TypeScript（额外语言）

目标：

- 作为 Top-20 外的额外语言单独审计和完成。
- 使用 TypeScript Program/TypeChecker。
- 支持 project references、workspace、path mapping、`.d.ts`、package exports 和 lockfile。
- 加强已有 Next.js、Express 或测试 family。
- generic package facts 可适用于 React，但 behavior-specific React support 必须有独立合同。
- TypeScript 完成不得增加 Top-20 计数。

## 6.9 Go

遵守 Go N1 ADR 的现有边界：

- Tree-sitter 只能 candidate generation。
- 权威路径优先使用隔离、opt-in、bounded 的：
  `go/parser`、`go/token`、`go/types`、build/constraint。
- 不执行 `go test`、`go generate`、cgo、项目 build。
- 不默认使用 `go/packages`、`go list` 或 gopls，除非现有 ADR 明确重新授权。
- 支持 `go.mod`、`go.sum`、module/workspace identity。
- 目标 family：`go.testing.test_function`，前提是与 ADR 和源码一致。
- build tags、generated files、cgo、module replacement 和 unresolved imports 保持 UNKNOWN。

## 6.10 PHP

遵守 PHP N1 ADR：

- 使用隔离的 mago-syntax candidate frontend 或当前 ADR 批准方案。
- 使用版本固定的 PHP parse/lint oracle，例如 `php -n -l`，但不得执行项目代码。
- nikic PHP-Parser 只能按 ADR 作为 differential/fallback 使用。
- 支持 `composer.json`、`composer.lock` 和 PHPUnit 静态模型。
- 目标 family：`php.phpunit.test_method`，前提是与当前合同一致。
- 不运行 Composer scripts、autoload side effects、PHP tests 或项目入口。
- magic methods、dynamic include、runtime container、generated proxy 保持 UNKNOWN。

## 6.11 Swift

遵守 Swift N1 qualification handoff：

- 先完成 SwiftSyntax 与当前 Swift toolchain 的兼容性资格审查。
- 先验版本可能为 SwiftSyntax 603.0.2 与 Swift 6.3.3；必须重新探测和记录。
- SourceKit 只能在独立资格审查通过后使用。
- 支持 SwiftPM 静态 metadata 和 `Package.resolved`。
- 目标 family：`swift.xctest.test_method`，前提是当前 ADR 一致。
- 不运行 build、test、plugin、macro expansion 或项目代码。
- macro、generated interface、conditional compilation、actor/runtime dispatch 保持 UNKNOWN。

## 6.12 Ruby

遵守 Ruby N1 ADR：

- 资格审查 Prism 和当前 CRuby 版本。
- 先验版本可能为 Prism 1.9.0、CRuby 4.0.6；必须重新探测。
- 支持 Bundler/Gemfile/Gemfile.lock/gemspec/RBS 静态 metadata。
- 目标 family：`ruby.minitest.test_method`，前提是当前 ADR 一致。
- 不执行 Ruby、Bundler、gemspec、Rake task 或测试。
- metaprogramming、method_missing、eval、autoload、monkey patch 保持 UNKNOWN。

## 6.13 Visual Basic

第一阶段只支持 VB.NET，不得声称支持 VB6。

目标：

- 明确产品文档中的 dialect：Visual Basic .NET。
- 使用 Roslyn VisualBasicCompilation/SemanticModel。
- 支持 `.vbproj`、MSBuild、NuGet 和 assembly metadata。
- 复用 .NET provider 基础设施，但保持 C# 与 VB 的语法/语言身份分离。
- 选择 MSTest、NUnit、xUnit 或 source-backed VB-specific family。
- late binding、COM、dynamic、generated code 和 Option Strict 差异保持 UNKNOWN。
- 在最终报告明确 VB6 不在本轮支持范围。

## 6.14 Delphi/Object Pascal

目标：

- 首先明确支持 dialect；不得声称支持所有 Pascal。
- 在 Delphi 和 Free Pascal 之间选择一个有证据的 bounded target。
- 评估 FPC `fcl-passrc`/`TSimpleParser` 或其他可安全隔离的前端。
- 支持 `.dproj`、`.lpi`、`fpmake` 或选定生态 metadata。
- 选择 DUnit、DUnitX 或 spec/implementation family。
- compiler directives、conditional defines、RTTI、property/event、unit initialization 保持 UNKNOWN。
- 若无法获得安全、许可和版本合适的 frontend，给出 `PROVIDER_UNAVAILABLE` 或 `LICENSE_BLOCKED`，不要创建假 parser。

## 6.15 Ada

目标：

- 评估 Libadalang supplied-buffer/unit-provider 模式。
- 支持 GPR/Alire metadata。
- 选择 AUnit 或 Ada package spec/body family。
- 不运行 GNAT build、elaboration 或项目代码。
- generic instantiation、representation clauses、preprocessing、toolchain extensions 保持 UNKNOWN。
- 若 provider 环境缺失，仍完成 source-backed qualification report 和边界测试。

## 6.16 Fortran

目标：

- 明确 free-form/fixed-form、标准版本和扩展边界。
- 评估 Flang、LFortran 或 fparser 的有界解析能力。
- 支持 `fpm.toml` 和批准的项目 metadata。
- 选择 pFUnit、module/procedure 或 interface family。
- 不执行 compiler、preprocessor 或项目程序。
- compiler extension、COMMON/EQUIVALENCE、preprocessing、implicit typing 和 build variant 保持 UNKNOWN。
- 不得声称一个 frontend 覆盖所有历史 Fortran dialect。

## 6.17 SQL

目标：

- 首先固定一个 primary dialect；优先根据仓库目标和 source-backed 证据选择，例如 PostgreSQL，但不能未经审查直接决定。
- raw parser 只能证明语法结构。
- 从 checked-in migration、DDL 和 schema artifacts 建立 RepoGrammar-owned catalog。
- 不连接数据库。
- 不执行 SQL。
- 不使用生产 credentials。
- 选择 migration、DDL object、query-shape 或 transaction-pattern family。
- dynamic SQL、stored procedure language、vendor extension、runtime schema 和 ORM-generated SQL 保持 UNKNOWN。
- 在 public output 中避免泄露 query literals 或敏感 schema 内容。

## 6.18 R

目标：

- 使用版本固定的 parse-only 能力，例如隔离调用 `parse(keep.source=TRUE)` 和 `getParseData`，绝不 eval。
- 支持 `DESCRIPTION`、`NAMESPACE`、`renv.lock`。
- 选择 testthat、Shiny 或明确的 function/test family。
- 不加载 package，不执行 `.Rprofile`，不恢复环境，不运行 tests。
- NSE、formula、S3/S4 dispatch、active binding、dynamic library loading 保持 UNKNOWN。

## 6.19 MATLAB

目标：

- 明确 MATLAB release/dialect 边界。
- 评估 bounded parser、Code Analyzer artifact 或可再分发的安全 frontend。
- 支持 project/toolbox metadata。
- 选择 `matlab.unittest` 或 class/function family。
- 不执行 MATLAB、Octave、script、toolbox installer。
- 不改变 MATLAB path。
- 不运行 `eval`。
- Simulink 必须作为独立范围，不得隐含包含。
- proprietary provider/license 不可用时，准确分类 `LICENSE_BLOCKED` 或 `PROVIDER_UNAVAILABLE`。

## 6.20 Assembly

目标：

- 必须固定 architecture、object format 和 syntax/dialect，例如 x86-64 ELF 加一个明确语法。
- 不得声称支持“所有汇编”。
- 评估 LLVM MC parse-only 或等价安全 frontend。
- 不汇编、不链接、不执行。
- 支持 directive、section、label、symbol reference、call/jump candidate。
- 选择一个有边界的 label/call/directive family。
- macro、include、conditional assembly、self-modifying behavior、runtime control flow 保持 UNKNOWN。
- 其他架构必须在矩阵中明确列为不在当前 bounded dialect 内。

## 6.21 Scratch

目标：

- 支持有界的 `.sb3` ZIP + `project.json` 格式。
- 使用安全 archive handling：
  - 文件数限制；
  - 解压大小限制；
  - compression ratio 限制；
  - path traversal 防护；
  - symlink 防护；
  - JSON nesting 和节点数量限制。
- 不运行 Scratch VM。
- 不运行 extension。
- 不下载远程 assets。
- 建立 sprite、target、block、event hat、opcode、stack 和 broadcast 的 owned IR。
- 选择 event-hat/opcode-stack 或 broadcast-handler family。
- custom extension 和 unknown opcode 保持 UNKNOWN。

# 7. Parallel Agent-Team Strategy

这是非平凡重大功能。若环境支持多 agent，必须使用并行 agent teams，但要保证所有权不重叠。

协调 agent 独占：

- shared domain model；
- registries；
- public schema；
- storage migration；
- shared documentation；
- shared completion matrix；
- integration branch；
- 最终 review 和 commits 集成。

子 agent 使用独立分支或 worktree，建议按以下 lane 分配：

- Lane A：Python、JavaScript、TypeScript、Rust。
- Lane B：C、C++、Java、C#。
- Lane C：Go、PHP、Swift、Ruby。
- Lane D：Visual Basic、Delphi/Object Pascal、Ada、Fortran。
- Lane E：SQL、R、MATLAB、Assembly、Scratch。
- Lane F：第三方依赖通用模型、provider contract、安全与 source-free 测试。

实际并发数受可用 agent slots 限制。优先并行完全不重叠的文件。若只有少量 slots，就分批运行。

要求：

- 每个子 agent 先读它需要的 authority docs 和 skills。
- 每个子 agent 有明确文件所有权。
- 子 agent 不得同时修改 shared registry 或 shared normative docs。
- 子 agent 必须提交 coherent commit，并回报 SHA、变更、测试、风险。
- 协调 agent 必须等待仍在运行的 agent。
- 协调 agent 必须逐个审查 diff 和逻辑。
- 不得盲目 merge 或选择冲突一侧。
- shared registry 集成必须串行完成。
- 可 cherry-pick 经审查的原子 commit，或把子分支合入 integration branch。
- 子分支永远不得进入 `main`。
- 不得因为一个 language lane 阻塞而停止其他独立 lane。

# 8. Phases

## Phase 0：仓库、Git 与评分隔离审计

执行并保存结果：

- `pwd`
- `git status --short --branch`
- `git branch --show-current`
- `git rev-parse HEAD`
- `git rev-parse main`
- `git rev-parse origin/main`
- `git merge-base main HEAD`
- `git log -1 --oneline --decorate HEAD`
- `git diff --check`

检查：

- 当前分支是否为专用分支；
- worktree 是否 clean；
- 是否存在用户修改；
- 当前 branch 是否从评分基线分叉；
- `main` 是否仍是 `86dba38...`；
- `origin/main` 本地 ref 是否仍相同；
- `.repogrammar/`；
- `.codegraph/`；
- 缺失文档引用；
- 未追踪大型文件；
- 现有 language/provider registry；
- 当前完整测试状态。

创建：

- baseline evidence；
- 语言能力初始矩阵；
- dependency/provider/tool availability matrix；
- branch/worktree ownership matrix。

若当前 checkout 无法安全隔离，建立独立 worktree；不得清理或覆盖原 checkout。

## Phase 1：预注册和设计 gate

在 outcome-driven implementation 之前写明：

- 每种语言的当前状态；
- 目标状态；
- authoritative provider；
- dialect/version；
- manifest/lockfile 范围；
- representative family；
- primary oracle；
- safety boundary；
- typed UNKNOWN；
- 预计修改模块；
- 前置依赖；
- failure classes；
- stop condition。

为第三方分析平台预注册：

- domain model；
- provider ports；
- package identity；
- external symbol identity；
- provenance；
- source-free serialization；
- compatibility strategy；
- migration requirements；
- CLI/MCP 是否需要变更；
- 不支持的行为。

先审查现有 ADR。只有在现有 architecture 无法容纳时才新增 ADR。

## Phase 2：实现 scoped prerequisites

优先顺序：

1. 通用 dependency/package/external-symbol/provenance 基础能力。
2. provider capability probe、timeout、resource limit 和 error taxonomy。
3. 当前已有 7 种 Top-20 语言的严格完成缺口。
4. TypeScript 额外完成。
5. Go/PHP/Swift/Ruby N1 lanes。
6. 共享 provider 复用价值高的 VB.NET。
7. Delphi/Ada/Fortran。
8. SQL/R/MATLAB。
9. Assembly/Scratch。
10. completion review 和整体审计。

不得为了“覆盖数量”复制空壳 adapter。一个 adapter 至少要有：

- 能力探测；
- 明确 provider/version；
- 真实输入；
- bounded output；
- typed failure；
- tests；
- docs；
- provenance。

## Phase 3：source/config/static sanity gate

每个 lane 在运行 provider 或集成前检查：

- provider/license 是否允许；
- 版本兼容；
- 输入不执行项目代码；
- config parser 不执行 DSL；
- path 安全；
- timeout；
- output size；
- deterministic behavior；
- source-free public shape；
- core type boundary；
- exact family anchors；
- lookalike 排除；
- UNKNOWN 路径。

未通过 gate 的 lane不得进入“完成”状态。应修复或形成 source-backed negative verdict。

## Phase 4：执行实现与测试

- 按 coherent slice 实现。
- 每个 code change 同步测试和文档。
- 在 workstream checkpoint 批量运行 targeted tests。
- 不要在每个小 helper 后运行全套测试。
- 对高风险 parser/provider/security boundary 可提前运行 targeted tests。
- 每个 lane 完成后进行 diff 和逻辑审查。
- 检查 public behavior、serialization 和 backwards compatibility。
- MCP 改动必须遵守 MCP contract skill。
- CLI 改动必须遵守 CLI skill。
- 不得擅自改变 v0.1 命令定位。

## Phase 5：依据 oracle 分析和分类

逐语言回答：

- parser/frontend 是否真实工作；
- provider 是否真实工作；
- provider 是否只是环境可用而未集成；
- package inventory 是否完整；
- external symbols 是否能解析；
- family 是否有 exact anchors；
- lookalike 是否被拒绝；
- dynamic tail 是否 UNKNOWN；
- source-free 是否通过；
- security tests 是否通过；
- completion gates 通过多少；
- 是否计入 Top-20。

任何测试失败都必须分类到具体 failure class，不能只写“有问题”。

每个 optimization 必须针对上一轮实际 failure class，不允许无关调参或堆代码。

## Phase 6：Artifacts、commits、final audit

必须生成或更新：

- 每种 Top-20 语言：
  `docs/reports/language-support/<language>-completion-review.md`
- TypeScript：
  独立 completion review。
- Top-20 program summary JSON，例如：
  `docs/reports/language-support/top-20-program-summary.json`
- 第三方生态覆盖矩阵。
- provider/tool/version/license matrix。
- dependency manifest/lockfile matrix。
- UNKNOWN taxonomy 和 resolution matrix。
- source-free evidence。
- security/performance review。
- exact prerequisite commit SHA。
- relevant product support docs。
- indexing、semantic-worker、dependency、architecture、module map、roadmap、testing 和 changelog 文档。
- v0.1 development plans。
- `.agents/memories/` 中允许更新的 durable context。

如果 `AGENTS.md` 或 `CLAUDE.md` 被修改：

1. 只编辑一个 canonical guide；
2. 立即执行：
   `cargo run --quiet --bin repo-guard -- sync-agent-guides --from <edited-file>`
3. 验证两者 byte-for-byte 相同。

不得为了让报告看起来完整而创建没有证据的 completion review。

# 9. Autonomous Overnight Loop

最多执行 5 个主要自主轮次。每个轮次必须包含：

1. 上一轮状态审计。
2. 候选 workstream 排序。
3. 预注册目标和成功 oracle。
4. RepoGrammar/CodeGraph preflight。
5. 安全和依赖 gate。
6. 实现或 source-backed qualification。
7. targeted validation。
8. failure classification。
9. 只针对真实 failure 的一项或多项保守优化。
10. diff review。
11. 文档和 evidence artifact。
12. coherent atomic commit。
13. 更新总矩阵。

建议轮次：

- Round 1：
  基线审计、通用第三方依赖模型、provider contracts、安全边界、初始矩阵。
- Round 2：
  Python、C、C++、Java、C#、JavaScript、Rust、TypeScript。
- Round 3：
  Go、PHP、Swift、Ruby。
- Round 4：
  Visual Basic、Delphi/Object Pascal、Ada、Fortran、SQL、R。
- Round 5：
  MATLAB、Assembly、Scratch、跨语言 integration、review、full validation、completion audit。

可以根据真实依赖关系调整，但必须在最终报告解释。

以下情况可以提前停止某个 lane：

- 已达到真实完成；
- architecture blocker；
- source-backed exhaustive NO_GO；
- provider/license/environment 无法安全修复；
- 继续会要求付费、凭据、push、release、系统级修改或执行不可信项目代码；
- 达到资源限制。

不要因为某个 lane 停止而终止整个 program。继续所有其他安全 workstream。

夜间期间不要要求人工在轮次之间选择方案，除非涉及：

- credentials；
- 付费；
- push/release/PR；
- 不可逆外部操作；
- 无法隔离的破坏性动作；
- 会影响评分基线；
- 两个互斥选择会实质改变产品范围，且仓库没有权威答案。

# 10. Optimization Discipline

允许的优化：

- provider capability probe；
- adapter isolation；
- parser error recovery；
- timeout/resource handling；
- deterministic normalization；
- package identity canonicalization；
- external symbol resolution；
- UNKNOWN 精细分类；
- exact anchor 提取；
- lookalike rejection；
- fixture coverage；
- source-free serializer；
- registry 去重；
- cache freshness；
- provenance；
- security hardening；
- diagnostic quality。

禁止的优化：

- 为 fixture 硬编码答案；
- 只为了测试通过而弱化 assertion；
- 把 UNKNOWN 改成 guessed true；
- 仅根据文件名或 package name 宣称 framework behavior；
- benchmark-specific hardcoding；
- 跳过 provider failure；
- 删除失败测试；
- 关闭 warnings；
- 降低 repo-guard；
- 绕开模块边界；
- 用 prose 替代实现证据；
- 在没有版本窗口的情况下声称 library contract；
- 为完成数量创建空 adapter；
- 运行用户项目代码来掩盖静态分析缺口。

# 11. Validation Plan

根据实际改动运行 targeted tests，并在最终 integration branch 上运行全部 required gates：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

还必须覆盖：

- 每种 provider 的版本探测；
- provider unavailable；
- malformed provider output；
- provider timeout；
- output-size limit；
- path traversal；
- symlink escape；
- archive bomb；
- oversized input；
- malformed manifest/lockfile；
- unsupported dialect；
- unresolved import；
- resolved import；
- stale provider result；
- conflicting provider result；
- deterministic rerun；
- source-free serialization；
- positive exact-anchor family；
- lookalike rejection；
- low-support family；
- parse-degraded family；
- generic third-party package without contract；
- version-window mismatch；
- alias/re-export；
- local workspace dependency；
- direct/transitive dependency distinction；
- standard-library vs external package distinction；
- public compatibility；
- storage round-trip；
- CLI/MCP schema tests if touched。

如果某条命令无法运行：

- 记录精确命令；
- 记录 stdout/stderr 或关键错误；
- 分类阻塞；
- 不得写“all tests pass”；
- 继续运行不依赖该阻塞的其他检查。

# 12. Performance Requirements

对新增 parser/provider 记录至少：

- 输入规模；
- wall time；
- timeout；
- peak/output size 的可获得指标；
- 文件数和 symbol 数；
- cold/warm behavior；
- malformed/adversarial input behavior。

必须设置有界限制。不得因为“语言支持”而允许：

- 无界递归；
- 无界 archive expansion；
- 无界 provider output；
- 无界 symbol graph；
- 对整个依赖缓存的隐式全盘扫描；
- provider hang；
- 静默高内存使用。

性能审查不要求虚假的跨机器 benchmark，但必须有可复现命令和明确输入。

# 13. Commit Policy

提交被授权，但仅限本次 feature/integration/child branches。

要求：

- Conventional Commits。
- 每个 commit 独立 coherent。
- code、tests、docs 同一个 atomic commit。
- 明确 staging paths。
- commit 前运行 `git diff --cached --check`。
- commit 前审查 staged diff。
- 不提交 secrets。
- 不提交下载缓存、provider binary、编译缓存、大型临时 artifact。
- 不提交失败的 partial work 并称其为成功。
- 有价值的 negative qualification 可以作为独立 commit，但必须包含 evidence、verdict 和后续边界。
- 不添加任何 AI attribution。
- 不 push。
- 不 merge 到 main。
- 不删除本次尚未进入 main 的 branches。

建议 commit 类型：

- `feat(domain): add language-neutral dependency identities`
- `feat(provider): add bounded semantic provider contract`
- `feat(python): qualify external symbol analysis`
- `feat(go): add bounded testing family support`
- `docs(language): record matlab provider no-go`
- `test(security): cover scratch archive limits`

实际 commit 必须反映真实内容，不要机械照抄。

# 14. Required Completion Reviews

每一种语言的 completion review 至少包含：

- language；
- rank；
- dialect/version；
- provider/frontend；
- provider version；
- discovery/config；
- manifest/lockfile；
- owned IR；
- external symbol support；
- Library Contracts；
- exact-anchor family；
- fixtures；
- UNKNOWN cases；
- source-free result；
- security review；
- correctness review；
- completeness review；
- performance review；
- nine-gate checklist；
- completion state；
- top20 counted yes/no；
- evidence paths；
- prerequisite commit SHAs；
- known risks；
- exact non-claims。

总 summary JSON 必须是机器可读，并为所有 20 种语言和 TypeScript 各提供一条记录。

如果 schema 已存在，复用并扩展现有 schema；不要创建互相竞争的 summary 格式。

# 15. Final Success Criteria

只有在以下全部满足时可报告 `TOP20_COMPLETE`：

- 冻结 Top-20 的每种语言都通过九项 gate；
- 每种语言至少 bounded_preview；
- 每种语言有 authoritative/bounded frontend；
- 每种语言有 typed UNKNOWN；
- 每种语言有真实 family 和 adversarial fixtures；
- 每种语言有 source-free 输出；
- 每种语言完成 four-part review；
- 每种语言有 completion review；
- 所有 prerequisite commits 可追踪；
- full repository validation 全绿；
- integration branch clean；
- `main` 和评分基线完全未改变。

只有在额外 TypeScript 也独立满足九项 gate 时可报告：

- `TYPESCRIPT_EXTRA_COMPLETE`

只有在以下条件满足时可报告：

- `LIBRARY_ANALYSIS_PLATFORM_COMPLETE`

条件包括：

- 统一 package/dependency/external-symbol/provenance model；
- generic no-contract package analysis；
- completed languages 的主要 manifest/lockfile 支持；
- provider-backed symbol facts 或明确 UNKNOWN；
- Library Contract version boundaries；
- source-free output；
- deterministic tests；
- security/resource tests；
- public compatibility；
- 完整文档和 evidence。

即使 generic platform 完成，也不得声称理解任意第三方库的全部运行时行为。

如果任一强成功标签不满足，必须使用：

- `PARTIAL_AUDITED_PROGRESS`

并精确给出完成数量、失败 gate 和 blocker。

# 16. Final Report Format

最终以中文给出完整报告，代码、命令、文件名和仓库文档保持项目现有语言。

报告必须按以下结构：

## A. Outcome

- Primary target achieved：yes/no
- `TOP20_COMPLETE`：yes/no
- Top-20 strict count：`N/20`
- `TYPESCRIPT_EXTRA_COMPLETE`：yes/no
- `LIBRARY_ANALYSIS_PLATFORM_COMPLETE`：yes/no
- 总体 verdict
- 为什么该 verdict 是证据支持的

## B. Scoring Isolation Proof

- 开始 branch
- 结束 branch
- integration branch
- child branches
- worktree paths
- start `main` SHA
- end `main` SHA
- start `origin/main` SHA
- end `origin/main` SHA
- 是否 push：必须是 no
- 是否 merge main：必须是 no
- 是否创建 PR/release：必须是 no
- 是否影响评分基线

## C. Autonomous Rounds

表格列出：

- round
- workstreams
- preregistered objective
- gate
- implementation
- validation
- failure class
- optimization
- artifact
- commits
- result

## D. Language Matrix

为全部 20 种语言和 TypeScript 列出：

- rank
- language
- dialect/version
- status
- gates passed
- provider
- package metadata
- third-party analysis
- family
- UNKNOWN
- completion
- evidence
- commit SHA
- blocker

不得省略任何语言。

## E. Third-Party Library Platform

说明：

- package identity
- dependency snapshots
- external symbols
- generic package behavior
- contract registry
- version windows
- provenance
- source-free handling
- security boundaries
- 已实现 ecosystems
- 已实现 contracts
- 明确 non-claims

## F. Changed Areas

列出：

- source modules
- tests
- fixtures
- docs
- ADRs
- plans
- memories
- schemas
- storage/migration
- CLI/MCP changes

## G. Validation

对每个命令给出：

- exact command
- result
- duration if available
- failure excerpt if failed
- failure classification

## H. Commits

每个 commit：

- SHA
- Conventional Commit subject
- branch
- purpose
- changed areas
- validation
- whether integrated into feature branch

再次明确：没有 commit 进入 main。

## I. Artifacts

列出：

- completion reviews
- JSON summaries
- provider probes
- performance evidence
- security evidence
- logs
- untracked large files
- temporary downloads
- hashes
- active processes/jobs

## J. Remaining Risks and UNKNOWNs

按严重程度列出真实风险，不得隐藏 partial implementation。

## K. Exact Claims and Non-Claims

分别列出系统现在能证明什么，以及不能证明什么。

## L. Single Highest-EV Next Action

只给一个最有价值的下一步，包括：

- target
- reason
- expected evidence
- blocker to remove
- estimated scope

不要在报告末尾建议 merge 到 main。不要执行 merge、push、PR 或 release。把经过验证的工作安全地保留在 `feat/top-20-language-library-support` 及其子分支中，等待维护者醒来后人工审查。
```
