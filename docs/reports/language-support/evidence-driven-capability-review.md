# RepoGrammar 语言能力证据报告

日期：2026-10-14。分支：`feat/evidence-driven-language-capability`。
候选功能提交：`3741afd435309fcb192ef1b577bc0d852bfe391e`。
结论：**LOCAL_VERIFIED_SCOPED_RETRIEVAL_GAIN / UNRELEASED**。

本轮完成了既有 Python FastAPI／pytest Family 的四个查询短语修复。
开发语料检索命中从 **30/47 到 34/47**；两个公开仓库的前瞻冻结问题从
**0/9 到 3/9**，新增命中全部为 FastAPI route。已测错误 Family 选择仍为零。
**新增语言、框架、Family 类型、语义 provider 均为零，严格 Top-20 仍为 0/20。**
这证明既有证据的查询可达性改善，不能称为完成一种语言或新增语义覆盖。
性能只有单机描述性证据，PHP 另有未修复的既存准入契约偏差。
三个公开命中指向同一个已有的 23-member FastAPI Family，不是三个新模式。

机器可读结果见 [summary JSON](evidence-driven-capability-summary.json)。
原始日志、公开源码快照、索引和二进制保存在
`/private/tmp/repogrammar-language-capability-20261014/`，未纳入 Git。

## 1. 实际前后能力矩阵

下表将发现／配置、owned units/IR、角色／Family、provider、UNKNOWN 和公共
CLI/MCP 分开。`D/C` 只证明发现和有限配置清单；`Owned` 不等于语义正确性。
Family 列描述已有源码及审计证据，并非本轮重新认证所有语言。
`Partial` 表示公共路径存在但完整语言认证矩阵尚未关闭；`Bounded` 只覆盖已声明子集。
全部语言的语义能力在本轮前后不变；Python 增加四个开发查询和三个公开仓库 route 查询命中。
PHP 的历史 8/9 不能当作当前 G1 已认证。

| Language | D/C | Frontend and owned units/IR | Roles / bounded families | Semantic provider | UNKNOWN / CLI-MCP readiness | ADR-0020 before → after |
| --- | --- | --- | --- | --- | --- | --- |
| Python | `.py`; root pyproject/setup | Host Python 3.10+ AST/symtable, owned | Official FastAPI, pytest, Pydantic, SQLAlchemy; separate previews | Type provider not integrated | Claim registry; partial | 5/9 → unchanged |
| C | `.c`/`.h`; compile database/vcpkg/Conan inventory | Tree-sitter, owned structural | No C-specific family | No Clang provider | Partial obligations/readiness | 2/9 → unchanged |
| C++ | C++ suffixes; shared C/C++ inventory | Tree-sitter, owned structural | GoogleTest, Catch2, doctest, Boost.Test, CppUnit | No Clang provider | Partial obligations/readiness | 3/9 → unchanged |
| Java | `.java`; literal Maven POM | Tree-sitter, owned structural | Spring, JUnit/TestNG, JPA, JAX-RS, Servlet | No javac/JDT provider | Partial obligations/readiness | 3/9 → unchanged |
| C# | `.cs` and `.csproj`; literal NuGet | Tree-sitter plus bounded XML, owned | ASP.NET, EF, xUnit/NUnit/MSTest, FluentValidation | No Roslyn provider | Partial obligations/readiness | 3/9 → unchanged |
| JavaScript | `.js`/`.jsx`; root npm/config | Bounded syntax, owned | Shared TS/JS exact-anchor families; React role only | Optional source-checkout TS worker; no shipped pinned compiler | Partial obligations/readiness | 3/9 → unchanged |
| VB.NET | `.vb`/`.vbproj`; literal NuGet | Declared-subset parser, owned | MSTest | None | Typed subset fallback; bounded | 8/9 → unchanged |
| SQL | `.sql`; path roles only | PG16/SQLite3 invariant DDL subset, owned | CREATE TABLE family; no migration-tool inference | None | Dialect/catalog/runtime unknowns; bounded | 8/9 → unchanged |
| R | `.R`/`.r`; DESCRIPTION/NAMESPACE/renv | Testthat-path subset; owned units, block IR kind abstains | testthat | None | Registry + standing binding/NSE unknowns; bounded | 8/9 → unchanged |
| Rust | `.rs`/Cargo manifests | Tree-sitter, owned structural | RepoGrammar self-dogfood, serde, thiserror, tokio, clap, axum, tracing | Cargo metadata project facts only; rust-analyzer absent | Partial obligations/readiness | 3/9 → unchanged |
| Object Pascal/Delphi | Pascal source; `.dproj` packages | Declared-subset parser, owned | DUnitX | None | Dialect/unit binding unknowns; bounded | 8/9 → unchanged |
| Scratch | No product discovery | Disconnected ZIP/JSON prerequisite; no product IR | None | None; product NO_GO | No product UNKNOWN/CLI/MCP lane | 1/9 → unchanged |
| Go | `.go`, go.mod/work; source read only for `_test.go` | Declared-subset parser, owned | Test/Benchmark/Fuzz declarations | None; Go commands unauthorized | Registry; product fixtures, incomplete pinned readiness audit | 8/9 → unchanged |
| PHP | Discovery/config implemented; prologue admission contract drift | Prologue-bearing declared subset, owned | PHPUnit | None | Registry + standing base/runtime unknowns; current G1 partial | Historical 8/9; current score not requalified |
| Swift | Swift source/SwiftPM inventory | Declared XCTest subset, owned | XCTest | None | Typed syntax/import/conditional fallback; bounded | 8/9 → unchanged |
| Ada | `.ads`/`.adb`; Alire inventory | Declared AUnit subset, owned | AUnit registration | None; Libadalang lane NO_GO | Dialect/overload/dispatch unknowns; bounded | 8/9 → unchanged |
| Assembly | Lowercase `.s`; no authoritative profile | Bounded lexical labels/IR only | No family | None | Mandatory profile uncertainty; no family readiness | 1/9 → unchanged |
| MATLAB | `.m`; mpackage inventory | Declared unittest subset, owned | Class-based unittest | None | Dialect/binding/subset fallback; bounded | 8/9 → unchanged |
| Fortran | Fortran suffixes/fpm; free-form subset only | Declared test-drive subset, owned | test-drive subroutine | None; Flang lane NO_GO | Registry + standing binding/registration unknowns; bounded | 8/9 → unchanged |
| Ruby | Ruby/Bundler inventory; runner-scoped test source | Declared Minitest subset, owned | Minitest | None | Syntax/lexical/ancestry fallback; bounded | 8/9 → unchanged |
| TypeScript (extra) | `.ts`/`.tsx`; root npm/config | Bounded syntax, owned | Express, runners, Next, Fastify, Prisma, Drizzle, Zod, Nest, Hono, Playwright | Optional source-checkout TS worker; no shipped pinned compiler | Partial obligations/readiness | 3/9 → unchanged |

实际边界保持：filesystem adapter 发现与读取；parser/worker 转换为 owned IR/facts；
framework adapter 识别角色；application 的统一策略决定派生支持、兼容性和 UNKNOWN；
SQLite adapter 持久化；query 层检索并验证新鲜度；CLI/MCP 只投影结果。
本轮改变 query normalization，端到端测试仍经过整个现有链路。

## 2. 选择 Python 的依据

优先顺序为 **Python → TS/JS → Go → Rust → Java → C#**，是当前证据和成本下的
工程顺序，不是已测客户需求排名。实际 RepoGrammar 需求／agent 查询频率均为
`NOT_MEASURED`。冻结开发语料有 32 个 Python 检索目标、10 个 TS/JS 目标及
5 个 Python 目录范围检索目标；Go/Rust/Java/C# 没有同口径检索分母。

| 候选 | 现实查询与可复用实现 | 未关闭机制、成本及风险 |
| --- | --- | --- |
| Python | 四个旧失败已定位为短语分数不足，目标 Family 已存在且新鲜；直接复用相邻双词表及完整产品路径 | 无新 runtime/dependency；主要风险是宽泛问题误选，须保持阈值和歧义拒答 |
| TS/JS | 已有精确 Family 与 10 个开发检索任务；可复用相同查询层 | compiler worker 未随产品发行；完整项目模型、包身份和不可信仓库隔离仍未完成 |
| Go | 已有 testing 声明子集及六组产品 fixture | 一种声明 Family；generics/cgo/Unicode 拒绝、环境和包语义限制仍在；无 Go 执行授权 |
| Rust | 自举及 serde/tokio/axum 等精确源码锚点 | item/type/trait/cfg/macro 语义需要另外准入的 provider；构建脚本和宏隔离成本高 |
| Java | Spring／测试／JPA 等既有 Family 和局部数据提供器 fixture | effective model、classpath/JAR、annotation processing 的真实性与隔离尚未闭合 |
| C# | 已有 controller/test Family；本轮核对确认 csproj 清单已持久化 | SDK/target/partial/generator/assembly 语义及 Roslyn 准入仍开放 |

外部需求代理：[Stack Overflow 2026 survey](https://survey.stackoverflow.co/2026/technology)
的受访者使用率为 JS 62.0%、Python 58.0%、TS 43.8%、Java 27.6%、C# 27.1%；
[GitHub Octoverse 2025](https://github.blog/news-insights/octoverse/octoverse-a-new-developer-joins-github-every-second-as-ai-leads-typescript-to-1/)
亦显示 TS/Python/JS 的广泛使用。这些不同总体的统计不能替代 RepoGrammar 需求或
可回答查询测量。没有据此推算 Go/Rust 百分比。Kotlin、Dart、Bash 保持后续候选。

本切片由冻结 [v0.6 handoff](../../plans/v0.6-adoption-retrieval-release-plan.md)
和 [17-miss audit](../../experiments/retrieval-miss-audit.md) 已明确列出，不扩大冻结范围。

## 3. 已验证查询及未新增的模式

`query-vocabulary.v3` 在原有三个相邻双词短语上新增四项：

| 新短语 | 开发语料恢复的查询 | 目标 |
| --- | --- | --- |
| `http handler` | `http handler` | 既有 FastAPI route Family |
| `rest endpoint` | `Where do we define REST endpoints?` | 既有 FastAPI route Family |
| `http route` | `Show me how HTTP routes are wired up` | 既有 FastAPI route Family |
| `writing test` | `What's the pattern for writing tests here?` | 既有 pytest-test Family |

使用现有大小写／复数折叠，不做模糊拼写、framework 猜测或阈值下降。
score floor **10**、margin **1**、exact-first 路由、新鲜度和兼容性门禁保持不变。
新 Family 类型数为 **0**；12 个完整 owned-analysis 比较也未发现新 Family。

## 4. 恢复和保留的 UNKNOWN

四个开发失败从 `below_min_score` 恢复到新鲜 Family 及 read plan；源码中原有的
运行时／框架语义 UNKNOWN 没有消失。开发语料其余 13 个 miss 保留：1 个裸概念
契约冲突、10 个粗粒度概念／真实 variant 歧义、2 个缺词且潜在歧义的目标。

公开语料仍有六个 pytest 问题未命中，全部在 `margin_too_close` 停止，margin 为 0，
没有 hydration。FastAPI template 有 7 个真实 pytest variant，Requests 有 4 个。
没有删除 Family、合并 variant 或改 gold 来获得命中。
裸概念、拼写相似、无对应服务器 route、过期／冲突证据、不支持的动态语义等仍应拒答。
`f-conflicting-languages` 是 OR 语言过滤下的裸概念控制，不是硬语言冲突。

## 5. 评估、来源和有效边界

| 指标 | 旧产品 | 候选 |
| --- | --- | --- |
| 开发语料整体匹配 | 93/110 | 97/110 |
| 开发检索 Hit@1 | 30/47 | 34/47 |
| 开发已选 Family precision | 38/38 | 42/42 |
| 开发错误 Family 选择 | 0/55 | 0/55 |
| 开发正确拒答／拒答 gold 被选中 | 36/36；0 | 36/36；0 |
| 开发 candidate Recall@5 | 18/19 | 18/19 |
| 开发 context | 27/27 | 27/27 |
| 公开语料整体匹配 | 11/20 | 14/20 |
| 公开正检索目标命中 | 0/9 | 3/9 |
| 公开已选 role-prefix precision | 未定义：0/0 | 3/3 |
| 公开错误 Family／正确拒答 | 0/9；11/11 | 0/9；11/11 |
| 公开 candidate Recall@5 | 未测量：gold 分母 0 | 未测量：gold 分母 0 |

另用冻结前缀与**旧完整 Family 清单**离线派生相关集合，保留原候选顺序，
只计算前五项。该补充为 `POSTHOC_DESCRIPTIVE_ROLE_PREFIX`：Micro Recall@5
从 **27/33（81.82%）到 28/33（84.85%）**，Macro Recall@5 从 90.48% 到
92.06%，Hit@5 两组均为 9/9。唯一变化是 `f-writing-tests-api` 的候选覆盖
从 3/7 到 4/7；它仍因歧义返回 UNKNOWN。相关集合包含同角色的全部既有变体，
不证明具体 session/cookie/auth 任务所需变体正确。原始主评分器的空分母、
问题、gold、阈值及结果均未改写，也没有为此重新运行产品。
补充产物 `supplementary-recall.json` 的 SHA-256 为
`04ae4e057b2371a431c2134cf5c461a43ef480449e0614d29ff9a4400014d368`。

公开 gold 只验证 Family role prefix，不验证 sessions/cookies/authentication/redirects
等具体编码任务是否得到充分答案。精确任务 precision、agent task success、采用率和
token savings 均为 `NOT_MEASURED`。FastAPI 仓库曾用于早期工作；问题在候选输出前冻结，
但评估者见过开发语料，因此这不是独立作者封存的泛化基准。

| 公开仓库 | 固定 commit | 保留的 tracked regular files | 省略链接 |
| --- | --- | --- | --- |
| fastapi/full-stack-fastapi-template | `4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8` | 236 | 4 个已校验 Git symlink blob |
| psf/requests | `0e322af87745eff34caffe4df68456ebc20d9068` | 129 | 2 个已校验 Git symlink blob |

保留所有固定版本的 tracked regular files；没有执行仓库代码。每个 Git blob、许可证、
归档和物化快照均有哈希。六个 symlink 按 path/mode/blob hash 记录并省略，未跟随或
替换；这是在产品评估前记录的协议补充，不是完整 symlink worktree。与原始 worktree
分析等价仅属推断，未实际测量。

完整捕获为 12 个分析、348 个 CLI/MCP 响应、174 个比较对；130 个受保护比较全部原始
字节相同，未使用允许的 vocabulary metadata 例外。共核验 228 个 read-plan 项，过期项 0。
这证明范围／哈希／行边界等机械可用性，不证明 agent 能正确完成任务。另一次全局 readiness
检查的六对结果只在预声明的 vocabulary version 路径不同。67 行既有 payload matrix 的
summary 字节相同，总计 330,956 bytes。

旧产品与候选都运行相同 structural/default 管线，worker 哈希不变。Provider 配置对照为
`NOT_APPLICABLE`，不能把未启用 provider 重命名为消融。另保留 token-overlap 诊断下界：
它在开发语料有误选及拒答错误，且其路由匹配／进程内时间口径不能与产品子进程直接比较。

## 6. 性能和安全

主测量为 macOS arm64、3 对交错顺序、18 次固定 harness 调用，全部正常退出。
下表为 wall ms 中位数 `[最小值, 最大值]`，每格 n=3；没有统计显著性、性能等价、
无回归或加速结论。正确性 grader 的并发计时全部排除。

| corpus / operation | 旧产品 ms | 候选 ms |
| --- | --- | --- |
| synthetic_n16 / cold init | 332.2 [294.7, 341.9] | 308.7 [308.2, 325.8] |
| synthetic_n16 / full resync | 257.2 [247.7, 275.2] | 230.2 [221.9, 247.6] |
| synthetic_n16 / zero-delta sync | 47.8 [44.9, 57.2] | 40.5 [40.0, 44.8] |
| fastapi-template / cold init | 609.7 [526.6, 622.8] | 592.8 [555.0, 634.2] |
| fastapi-template / full resync | 527.4 [492.7, 545.9] | 505.1 [502.4, 574.5] |
| fastapi-template / zero-delta sync | 110.2 [101.8, 112.9] | 116.5 [101.7, 123.0] |
| requests / cold init | 988.2 [966.4, 1115.8] | 1014.3 [982.4, 1082.5] |
| requests / full resync | 910.5 [908.3, 972.0] | 929.2 [910.5, 934.8] |
| requests / zero-delta sync | 129.9 [123.2, 144.0] | 135.8 [123.1, 137.2] |

Synthetic Python body-edit sync 为 265.0 [251.7,280.0] → 237.9 [219.9,245.8] ms；
project-config edit 为 266.4 [266.1,300.1] → 281.8 [258.9,298.9] ms。
其余编辑类的完整样本保留在 JSON。所有主测量对的 portable counters 和 owned fact/IR
shape 均相同。cold-init native peak RSS 中位数分别为 synthetic 39.56→39.75 MiB、
FastAPI template 40.30→40.23 MiB、Requests 46.36→46.44 MiB。
RSS 是 `/usr/bin/time -l` 的 native child rusage maximum，不是同时存活进程树的 RSS 总和。

Requests 主测量 exact-family 为 67.88→77.19 ms，MCP readiness 为 144.61→163.64 ms，
出现约 +14%／+13% 差异。随后按预先固定的 old×3、candidate×3、candidate×3、old×3
执行四次**探索性诊断**，原始 18 次主测量不被替换：

| Requests 诊断（每臂 n=6） | 旧中位数 [范围] ms | 候选中位数 [范围] ms | 描述性变化 |
| --- | --- | --- | --- |
| exact-family CLI | 74.13 [72.49,80.23] | 71.42 [68.90,123.78] | −3.66%；两个 block pair 同向 |
| MCP readiness | 143.56 [139.05,150.08] | 145.40 [140.28,163.81] | +1.28%；pair 为 +5.11%／−0.94% |
| fuzzy CLI | 98.29 [92.51,138.55] | 102.90 [93.05,111.67] | **+4.69%**；仍有未归因差异 |

初始较大差异没有一致重现，不能据此宣称没有性能回归。fuzzy CLI 的 +4.69% 必须保留；
因果归因和显著性未建立。Linux、其他架构、代表性大规模 provider 开销未测量。

产品没有增加依赖、provider 或仓库代码执行；受保护输出没有新差异。
这不等于全面安全认证。发现的既存风险是
[PHP 准入 OPEN CONTRACT_DRIFT](php-completion-review.md#2026-10-14-open-admission-contract-drift)：
ADR-0047 D1 要求非 prologue PHP 保持 source-store 前零读取，但实现先读后在 parser 拒绝。
本轮未修复或修改 ADR，也不能因测试通过而关闭该要求。

## 7. ADR-0020 门禁

严格完成度仍为 **0/20**；TypeScript extra 独立报告。Python 仍为 5/9，其他历史计数见
能力矩阵，任何计数都不因 query 修复而增加。PHP 旧 8/9 是历史审计；当前 G1 为
`PARTIAL / NOT_REQUALIFIED`，具体 D1 零读取子声明被源码路径反证，尚未执行专门
sentinel 修复认证。不存在全语言／provider 完成声明。

C# `.csproj` 已贯通 discovery→parser→SQLite／incremental，旧“未接 discovery”文档已纠正；
完整测试中对应回归明确 PASS，但 C# 仍为 3/9。架构、domain model、indexing pipeline 和
product scope 中其余旧 inventory-only 描述按已有 ADR 对齐；这是审计纠偏，不是新增批准。

## 8. 分支、提交、验证和产物

功能提交：`3741afd435309fcb192ef1b577bc0d852bfe391e`。
冻结评估工具与测试提交：`450c87c55571f446af2e9e8f948c6d4aa942cd7d`。
离线候选召回补充提交：`536b80a`；安装测试 PATH 修复提交：`9271fad`。
旧基线为 `4916f57b2d4a22534a885ce18dcafb3710e74d53`；旧 binary 建于 `3da8cee`，
Coordinator 已核验两者 `src/`、Cargo manifests／lockfile 等价。
候选源码清单已按功能提交核对，捕获工具哈希已按评估提交核对。
文档审计使用另一个原子本地提交，由最终交付记录列明。
没有 push、tag、合并到 main 或 publish；预检仅将尚无改动的功能分支快进到
当时更新后的远端基线。本轮运行任务均已结束。

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace --all-features` | 2,590 PASS；2 个既有 benchmark ignored |
| Python／TypeScript worker tests | PASS |
| `repo-guard check`、`check-geo` | PASS；GEO 不是收录效果证据 |
| `repo-guard sync-equivalence --all` | 旧／候选各 14/14 PASS |
| 原 110-query product-eval、公开 20-query product-eval | 按第 5 节报告保守 miss，未修改 gold |
| CLI/MCP capture、payload matrix、global readiness | 第 5 节对应门禁 PASS |
| prepare／capture／canonical harness selftests | 5／6／5 PASS |
| 离线 Recall 补充 selftests | 5/5 PASS |
| installer tests | 完整兼容套件 PASS；最终独立真实候选 E2E 34/34 PASS、0 跳过；内嵌升级缺省跳过项由独立运行覆盖 |
| npm wrapper、`npm pack --dry-run --json`、packaged-artifact smoke | PASS |
| JSON、相对链接、`git diff --check`、AGENTS/CLAUDE 镜像 | 本轮核验；最终文档门禁由 Coordinator 收口 |

完整 Rust 计数为 library 2,286、repo-guard 118、product 185、doctest 1。
保留初始 RED／失败日志及修正后的 final 日志，没有删除失败证据。
本机 packaged smoke 不证明四平台 release 或公网发行；跨修订升级对照使用保留的旧源码
candidate，不是公开发行版升级认证。

预检后再次读取[上游基线 CI](https://github.com/SioYooo/RepoGrammar/actions/runs/38515721826)：
`4916f57` 的 Rust、macOS smoke 和 Linux packaged lifecycle 通过，但 quality 在
Linux installer fixture 失败，确切日志为 `tar (child): gzip: Cannot exec`。
测试隔离 PATH 漏放已有的 gzip 前置工具；macOS tar 内置解压掩盖了缺口。
`9271fad` 只补齐测试 PATH 并添加真实归档校验回归：旧夹具在本机复现
`FileNotFoundError: gzip`，修正后完整真实候选 34 项 E2E 通过。安装器／产品代码未改。
该远端 CI 仍是失败记录，Linux rerun 尚未执行；本地通过不等于 CI 已恢复。
同一运行的 Windows source-only 诊断也失败，workflow 明确 `continue-on-error`；
Windows 修复／支持扩展仍不在授权范围。新功能分支未推送，未触发新 CI。
保留了原始失败日志、成功复验及临时证据，未进行破坏性仓库清理。

| 身份／主要产物 | SHA-256 |
| --- | --- |
| 原预注册 `preregistration.json` | `9854a75da31e451b78b6ba5d46905fef98cc905bb07dac1719cf1cdf7f0c2e64` |
| `qualification-controls.json` | `55aaf48f47d58d83dc900047f3ddea3e2eb5a0c769462bf66abd7123a4716fe1` |
| 旧 binary | `e932069a934650d68b93b1c53bfbd7df63a4cbc72616e56e9b2c62db3ad28932` |
| 候选 binary | `e0dc74604ab27a357a6682a5110b6f4998a33f505f065c0aed4d03939247f7a8` |
| 共同 Python worker | `c25552c77b237eb330287d299224c821448a44fd8eca017e161b45fe45f1e5bc` |
| `evaluation-summary.json` | `4e8d94f833d8a92fe5d4423e47f2f97335f22a86508629f4bc65fa446bc771d4` |
| `requests-stability/summary.json` | `6764a794bfc434fcd0a671bcfdaedc6af69935533847298cda8905c67166f091` |

JSON companion 保留各 primary artifact 的路径／哈希、全部样本、来源和协议界限。
文档 Reviewer 已独立比对六个 quality artifact、capture、readiness、两套 sync 的哈希及
关键 primary 计数；性能诊断独立保留 plan/hash，不覆盖主测量。

## 9. 下一项最高价值工作

先为**既有 pytest variant 的可证明缩小查询范围**登记独立方案：检查 path、symbol 或
现有 constraint profile 能否把真实多 Family 候选缩小到唯一可选项；冻结正、负、tie 和
stale 样例，再验证新 read plan 是否有用。保留 score/margin 和 variant 边界，不能仅给
`writing tests` 加分来强选六个真实歧义目标。新的语义机制仍需独立规格／准入。

PHP D1 的 admission 修复是单独的风险处理项，必须明确 source-read 边界并补产品级
non-prologue sentinel；不能混入上述检索优化或通过改低级文档取消 ADR 要求。
Kotlin、Dart、Bash 不承诺实施。

## 预注册保留记录

预注册于 2026-10-14、候选实现前针对 `4916f57` 冻结；原文件及 110-query gold 不变。
四短语、score 10、margin 1、零新 Family／provider、无仓库代码执行为冻结约束。
公开快照 symlink 处理和标签解释只存在于有独立哈希的 additive controls 中。
主测量与探索性 Requests 稳定性诊断严格分开，未把不利样本改写为正结论。
