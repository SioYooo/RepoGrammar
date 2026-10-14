# RepoGrammar one-command quickstart 验证报告

结论：`LOCAL_VERIFIED / UNRELEASED`。代码、隔离端到端测试和本机打包门禁已完成；公网一行命令尚未上线。

## 基线、分支和参考调查

起始基线为 `aaf5444c3ad44c6c4e3f8f42b8f09b90658c1a9e`，当时与本地
`main`、`origin/main` 一致。功能分支为 `feat/one-command-quickstart`。
已有简化分支保留在 `4ed1ff4aa74fef45d592ce8cb4dc99f31bf8eb72`，未覆盖或合并。
实现提交：`3da8ceecb1e8f4c8d8988ccfecd7a9367279e726`。

CodeGraph 调查固定到 `34dd3d2fb4e15199d313f36450b095acf32dcb14`，
其 [package.json](https://github.com/colbymchenry/codegraph/blob/34dd3d2fb4e15199d313f36450b095acf32dcb14/package.json)
为 1.6.2。检查了
[install.sh](https://github.com/colbymchenry/codegraph/blob/34dd3d2fb4e15199d313f36450b095acf32dcb14/install.sh)、
[install.ps1](https://github.com/colbymchenry/codegraph/blob/34dd3d2fb4e15199d313f36450b095acf32dcb14/install.ps1)、
[安装编排](https://github.com/colbymchenry/codegraph/blob/34dd3d2fb4e15199d313f36450b095acf32dcb14/src/installer/index.ts)、
[CLI 接线](https://github.com/colbymchenry/codegraph/blob/34dd3d2fb4e15199d313f36450b095acf32dcb14/src/bin/codegraph.ts#L2690)
和 [quickstart](https://github.com/colbymchenry/codegraph/blob/34dd3d2fb4e15199d313f36450b095acf32dcb14/site/src/content/docs/getting-started/quickstart.md)。

| 操作 | CodeGraph 的已读实现 | RepoGrammar 原有能力/缺口 | 本次处理 |
| --- | --- | --- | --- |
| 获取 CLI | shell 获取独立包 | 已有四平台预编译包、checksum 和 worker；无 quickstart | 复用下载，增加明确入口与能力检查 |
| 连接代理 | CLI installer/registry；部分默认权限和 hook 行为 | 已有三个 live global writer 和 receipts | 复用 Rust；不复制权限放宽、hook 或遥测默认值 |
| 初始化 | CLI `install --init` 复用 init | 已有 setup；缺少 none/machine-only 路径 | shell 调 setup，增加显式跳过边界 |
| 更新/PATH | 版本目录管理和 PATH 提示 | 已有产品所有权；旧 wrapper 有 PATH 清理 | quickstart 严格验证所有权，保留其他 PATH 命令 |
| 卸载 | 独立删除流程 | 已有受管收据和 post-exit finalizer | 沿用权威，仅补齐无原生客户端时的窄例外 |

CodeGraph shell 不是完整接入向导的唯一实现；RepoGrammar 没有照搬其删除、
默认遥测或自动权限策略。当前公开
[v0.5.0](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0)
仍是不可变旧发行版，installer 和 binary 都不提供新 bootstrap 协议。

## 最终流程与架构

```text
shell 临时下载/校验
  -> 候选 binary 的 quickstart-v1 能力与 Rust 所有权预检
  -> 现有 setup dry-run + 实际 Git 根目录/安装路径
  -> /dev/tty 明确确认，或 --yes
  -> 现有 Rust 安装事务（binary、command、worker、receipt、自检）
  -> 核对原计划指纹，再执行既有 setup
  -> 分别报告代理、索引、autosync、MCP 自检和恢复动作
```

源文件分工：

- `src/install/repogrammar-install.sh`：bootstrap、平台/归档校验、TTY、计划展示和委托。
- `application/install.rs`、`product_installation.rs`：复用命令安装事务、收据权威和回滚；没有 Bash 收据解释器。
- `application/setup.rs`、`interfaces/cli/mod.rs`：none/machine-only、计划绑定、代理状态及真实 next action。
- `application/query.rs`：仅将现有 freshness 投影向 crate 内共享，算法未改。
- `bin/repogrammar.rs`：私有获取入口和既有产品 MCP self-test 接线。
- `src/install/quickstart.test.py`、原 shell 测试：隔离实际候选包、代理协议替身和失败注入。

`--quickstart` 支持 `--yes`、`--dry-run`、`--project`、`--skip-repository`、
`--target auto|none|codex|claude-code|opencode`、`--scope global`、
`--no-autosync`、`--no-instructions`、版本固定与显式未受管命令替换。
local/project live writer 未实现，因此明确拒绝并给出手动配置指导。
没有源构建、Node/npm、Docker、模型调用或自动安装 Python。

## 实际终端证据

以下节选来自真实候选的管道 + controlling PTY 测试；测试显式使用
`--no-autosync`，未配置代理，路径已缩写，进度行省略：

```text
Python analysis runtime: 3.14.6
Verifying RepoGrammar latest for aarch64-apple-darwin
Quickstart installation plan
  Managed installation: <isolated-home>/.local/share/repogrammar
  Command: <isolated-home>/.local/bin/repogrammar
  Agent scope: global; selection: auto
  Repository: <fixture-project>
- agent MCP wiring: unavailable; repository setup can continue
- repository initialization: will run
- repository indexing: will run
- autosync: disabled
Apply this installation and setup plan? [y/N] yes
CLI installation verified; product MCP self-test passed.
setup: completed with limitations
repository: 1 files indexed, 1 pattern groups verified
limitation: no supported live agent CLI was detected
product MCP: repogrammar_context self-test passed
agent MCP: not active; use the repository index through the RepoGrammar CLI
next: install a supported coding agent, then run repogrammar setup
```

另一项真实测试使用默认 autosync，验证 running、停止及锁清理。
安装器从 stdin 执行不读取脚本流作为回答；无 TTY 且无 `--yes` 时零持久写入。
确认期间新增代理的测试证明：计划变化后只保留已安装 CLI，拒绝额外代理/仓库写入。

## 验证结果

最终候选 binary SHA-256：
`e932069a934650d68b93b1c53bfbd7df63a4cbc72616e56e9b2c62db3ad28932`。
Bundled worker SHA-256：
`c25552c77b237eb330287d299224c821448a44fd8eca017e161b45fe45f1e5bc`。

本机是 macOS arm64，Python 3.14.6。系统 Python 3.9 未用于成功门禁。
Rust/Cargo 1.96.0、Node 22.23.1 仅用于开发验证，不是二进制安装用户的要求。

| 门禁 | 结果 |
| --- | --- |
| 实际候选直接/管道/PTY E2E | 33/33 PASS，0 跳过，50.464 秒 |
| 原 shell installer 完整兼容套件 | PASS；内嵌 32 PASS、1 个旧 binary 参数缺省跳过，独立 E2E 已覆盖该升级 |
| Rust workspace | library 2,281；repo-guard 118；product binary 185；doctest 1 全通过；2 个原有 opt-in benchmark 保留忽略 |
| fmt / clippy `-D warnings` | PASS |
| Python / TypeScript worker | PASS |
| npm wrapper / local pack dry-run | PASS；未发布 npm |
| exact archive packaged smoke | PASS，包含真实 autosync 和 `--require-product-uninstall` |
| repository guard / diff gate / guide equality | PASS |

E2E 覆盖 clean/single/multiple/no-agent、outside Git、显式项目、fresh repeat、
真实 stale refresh、旧源码候选升级、foreign/malformed/drifted 配置、命令冲突及备份、
PATH 副本、Python 缺失/旧版/4.x、十类归档失败、下载中断、代理/索引失败、权限/链接、
autosync、最终卸载和计划变化拒绝。33 个案例及原始产物哈希见
[summary JSON](one-command-quickstart.summary.json)。

代理检测/配置测试使用隔离协议替身；opencode 使用真实文件 writer。候选 binary、
worker、索引、MCP server 自检和所有权生命周期都是真实执行。没有把模拟代理当作
真实 Codex/Claude/opencode 宿主 tools/call 验证。

升级对照使用保留的旧源码候选（SHA
`dd394e86596e93adadd945276a626701ca1fa8ab09eb3d4e6769a3c06f9dbe5a`），
证明不同 binary 内容的受管替换，不等同于公开发行版升级证据。

## 发现、修复和安全边界

独立审查和真实运行推动修复：

1. 确认后的重新规划可能扩大代理范围：计划指纹绑定并在写入前比较。
2. 合法 checksum 无末尾换行被误拒绝：保留严格 digest/name/hash 验证并接受 EOF。
3. macOS 临时路径别名：只将新建私有 staging 目录规范化；安装目的地链接仍拒绝。
4. setup 只看 dirty markers：复用已有家族来源哈希 freshness，保留无家族 fastpath。
5. 无代理 CLI 无法卸载：仅无 CLI 且收据/备份严格缺失时跳过，报告 UNKNOWN/preserved；
   任何现存 PATH 候选或非 NotFound 错误都进入严格探测，执行前再次检查。
6. `--target none` 的下一步错误要求询问代理：改为 CLI `find` 指导。

产品获取失败恢复本次事务的旧 bytes、权限和 receipt；setup 只回滚新建代理集成。
既有集成、已激活 generation、外国文件和 PATH 命令保留。无法证明回滚所有权时
保留文件并报告人工恢复，不扩大删除范围。并发同目录恶意 rename/path-swap 的既有
限制没有被宣称解决。

保留了初始失败记录：测试隔离 PATH 漏放 `kill` 导致正确的 liveness UNKNOWN；
卸载测试曾只等 report 存在而未等 `in_progress` 结束。两项修正均只改测试环境/轮询，
没有放宽产品判断。沙箱 TTY 访问限制单独复核后使用原生隔离运行。

## 精确命令与复现

本机验证使用 `PATH=/opt/homebrew/bin:$PATH`，以及
`REPOGRAMMAR_PYTHON_EXECUTABLE=/opt/homebrew/bin/python3`。
完整 argv、日志哈希和真实 artifact 身份在相邻 JSON；所有 HOME、Git 仓库和代理状态
均为临时隔离夹具。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
REPOGRAMMAR_INSTALLER_TEST_BINARY=<frozen-candidate> bash src/install/repogrammar-install.test.sh
python3 src/install/quickstart.test.py --binary <frozen-candidate> --previous-binary <frozen-previous> --out <new-evidence-dir>
node src/npm/repogrammar.test.js
npm pack --dry-run --json
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD
git diff --check origin/main...HEAD
cmp -s AGENTS.md CLAUDE.md
```

打包门禁使用从最终 E2E release fixture 解出的原始 binary/worker；校验二者与冻结
候选逐字节相同后调用原有 `smoke-packaged-artifact`，参数包含
`--expected-version 0.5.0 --require-product-uninstall`。这里的 0.5.0 是当前源 manifest
身份，不是把新代码写入不可变公开 0.5.0。

## 尚未发布的事项

目标公网命令是以下形式，当前不作为已上线能力推荐：

```sh
curl -fsSL https://raw.githubusercontent.com/SioYooo/RepoGrammar/main/src/install/repogrammar-install.sh | bash -s -- --quickstart
```

发布前需要单独授权：选未占用的新版本并同步源 manifests/稳定版本提示及文档，
整合并推送审阅后的源码，四个 native runner 完成 build/ABI/packaged/quickstart 门禁，
保留同次构建的十个原始资产及 checksums，再发布新不可变 tag/release 并验证公网字节。
优先使用固定 release-hosted installer，另保留先下载并核验 bootstrap 的方案。
同渠道 SHA-256 不是独立的发布者真实性证明；bootstrap 本身先于归档校验执行。

Linux x86_64/arm64、macOS x86_64 native runtime 及真实代理宿主均未运行；Linux libc
检查只有模拟/合同测试证据。本次未提供可验证该未推送提交的其他 native runner；未启动远程 workflow。
没有 push、merge、tag、release、npm dist-tag、真实全局安装或真实用户配置修改。
测试 daemon 均已停止，原始失败与最终证据保留在任务临时目录。
