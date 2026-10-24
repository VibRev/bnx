# bnx

面向 agent 的 Binary Ninja CLI：会话常驻在 daemon 里，提问写成 Python 脚本。命令、会话、daemon、输出都来自 [rex](../rex)（约定见 [rex CONVENTIONS](../rex/docs/CONVENTIONS.md)，建立在 [axkit CONVENTIONS](../axkit/docs/CONVENTIONS.md) 之上）；本仓库只有 Binary Ninja 自己的部分：worker 里的后端、Python console 与 prelude、`ref` 语料和帮助文本。

**引擎版本**：只支持 Binary Ninja **6.0 R2**（`stable/6.0.10905`，API 提交 `775c3b5e`）。worker 启动时检查加载到的 core 版本，与构建目标不符就报 `engine_error`（rex R-07）。

## 构建

```sh
export BINARYNINJADIR=/path/to/binaryninja                         # Linux：6.0 R2 安装目录
export BINARYNINJADIR=/Applications/BinaryNinja.app/Contents/MacOS   # macOS：app 里放 dylib 的目录
cargo build --release
```

- macOS（实测 macOS 15 / Apple Silicon）：bindgen 用 Xcode 的 libclang；Python API 与示例在 app 的 `Contents/Resources/` 下，`ref` 会自动找到。license 在 `~/Library/Application Support/Binary Ninja/license.dat`。

- `binaryninja` / `binaryninjacore-sys` 以 git 依赖固定在 `775c3b5eed8598217b20e1087afe7aa092a9b4b1`，与安装目录里的 `api_REVISION.txt` 一致。
- `build.rs` 把可执行文件链接到 `libbinaryninjacore` 并写入 rpath，运行时不需要 `LD_LIBRARY_PATH`。
- 运行需要 headless 可用的 license（Linux 为 `~/.binaryninja/license.dat`，macOS 为 `~/Library/Application Support/Binary Ninja/license.dat`，或 `BN_LICENSE_FILE`）。`--help` 和 `ref` 不需要。

```sh
cargo clippy --all-targets
cargo test        # 端到端测试需要 license，没有时自动跳过；fixture 规则同 idax（rex/fixtures/bin 预编译兜底）
cargo fmt --check
```

## 命令

命令与全局 flag 两个工具完全相同，见 rex README；环境变量前缀是 `BNX_`。`save` 默认写 `<target>.bndb`，target 是 `.bndb` 时覆盖它自己，并打印 `saved | <path>`。会话行：

```
# target | analysis | idle
/work/fw.bin | complete | 0s
```

## skill

`skills/bnx/SKILL.md` 是给 agent 的 skill，打包进二进制。bnx 只负责把它交出来，不安装：各家 harness 的 skill 目录不同，而且一直在变，由 agent 按自己所在的 harness 装最可靠。

```sh
bnx skills                 # 原样打印 SKILL.md，不截断（agent 模式下也是）
bnx skills list            # skill 的全部文件
bnx skills show SKILL.md   # 其中一个文件
```

frontmatter 的 `metadata.version` 与 `bnx --version` 相同，agent 拿已装副本的版本和它比就知道是否过期。`tests/cli.rs` 的 `every_skill_example_runs` 和 `every_ref_example_runs` 把 SKILL.md 与 `ref` 语料里每条示例在共用 fixture（`rex/fixtures/fw.c` 编出的 `fw.bin`）上逐条执行并检查退出码；改 skill 或语料后跑它们。

## ref 与引擎自己的文档

`ref` 语料只放 bnx 自己的东西：`exec`（结果语义、脚本、错误）、`session`、`pitfalls`（改名、headless 下的坑）。Binary Ninja 的 API 参考和官方示例不打包，而是在运行时实际加载的那份安装里现查：

- `bnx ref <words>` 逐行解析 `<BNDIR>/python/binaryninja/**/*.py`（约 5700 个定义，一次 25 ms 左右，不缓存），每条命中给出 `限定名(签名) -> 返回 | 绝对路径:行号 | docstring 首行`，名字唯一或精确命中时附完整 docstring；最后附相关的官方示例。查询是标识符（含 `_`、`.` 或大写）时 API 排在前面，普通词（`rename`）先给语料里的笔记：

  ```
  $ bnx ref get_functions_at
  binaryninja.BinaryView.get_functions_at(addr: int) -> List['_function.Function'] | <BNDIR>/python/binaryninja/binaryview.py:6076
      get_functions_at get a list of Function objects (one for each valid platform) that start at the
      ...
  $ bnx ref rename
  # ref pitfalls: rename a function
  Assign the name: func.name = "new_name" (there is no rename call); ...
  ```
- 不带参数的 `bnx ref` 在主题列表后列出安装里的位置，附可直接粘贴的 grep：`<BNDIR>/python/binaryninja`（签名与 docstring）、`<BNDIR>/examples/python`（官方示例；`api-docs/` 是几十 MB 的 HTML，不列）。找不到安装时一行 `hint:` 说明原因与 `BINARYNINJADIR`。

## exec 里有什么

```sh
bnx exec fw.bin 'len(bv.functions)'
bnx exec fw.bin 'str(bv.get_functions_by_name("main")[0].hlil)'
bnx exec fw.bin - <<'EOF'
[{"addr": hex(f.start), "name": f.name} for f in bv.functions if "crypt" in f.name]
EOF
```

- `bv` 是会话的 BinaryView，`from binaryninja import *` 已执行。不加 helper：原生 API 就够，不确定时 `bnx ref <words>`。
- 每次 exec 的全局变量都是新的。结果是最后一个表达式的值；不是 JSON 的对象输出 `str()`。
- 取消：客户端断开时 daemon 发取消，worker 调 `BNCancelScriptInput`，正在执行的 Python 代码收到 `KeyboardInterrupt`，会话保留。**阻塞在 C 里的调用（`time.sleep`、长时间的 core 调用）打断不了**：console 不在主线程上跑 Python，信号送不到；10 秒宽限期后 worker 被杀，会话丢失。

## MCP

`bnx mcp` 是 stdio MCP server，code mode，只有五个工具：`exec {target, code, timeout?}`、`ref {query}`、`save {target, path?, force?}`、`close {target}`、`list {}`，每个都是同名命令。它和 CLI 共用同一个 daemon 和会话：MCP 里打开的 target，命令行 `bnx status` 也看得到。输出与 agent 模式的 CLI 一样有界（约 8 KB，超出写进 spill 文件，最后一行给出路径），报错同样是 `Error:` 加 `hint:`，工具结果标 `isError`。工具定义合计约 2 KB。

```sh
claude mcp add bnx -- bnx mcp                  # Claude Code；或项目里的 .mcp.json：
                                                # {"mcpServers": {"bnx": {"command": "bnx", "args": ["mcp"]}}}
```

```toml
# ~/.codex/config.toml
[mcp_servers.bnx]
command = "bnx"
args = ["mcp"]
```

## 后端

- 每个会话一个 `bnx __worker` 子进程（rex 管理），daemon 本身不初始化 core。理由：
  1. **崩溃隔离**：core 或脚本崩溃（segfault、`os._exit`、OOM）只带走一个会话；
  2. **超时必须能杀**：卡在 core 调用里的 Python 中断不了，只能杀进程；
  3. **Python 解释器是进程级全局状态**：一个进程一个 view，会话之间不共享 `sys.modules` 和全局变量；
  4. daemon 不碰 core，`ref`、`--help`、`list` 不需要 license。
- 打开 `.bndb` 时由 rex 先复制进会话的工作目录再打开副本。`save` 每次都对目标旁边的临时路径 `create_database`，再由 rex rename 到位。
- scripting provider 的 console 按 REPL 编译输入，所以只给它单行调用 `__bnx_run(<请求文件>)`；脚本和结果走工作目录里的 JSON 文件，结果经 rename 出现即完成。每次提交前等 console 进入 `ReadyForScriptExecution`：机器繁忙时解释器启动慢，或上一次提交还没完全返回，这时提交会被当成无效输入拒绝（并行测试里偶发 `engine_error` 的原因）。
- 分析完成度按脚本开始时的状态报告；Binary Ninja 在后台分析。

## 目录

```
src/main.rs              Tool 描述（名字、帮助、语料），入口 rex::tool::main
src/backend/mod.rs       rex::worker::Backend：headless 初始化、版本检查、加载、分析等待、保存
src/backend/console.rs   python3 scripting provider 的 C API
src/backend/prelude.py   Python prelude
src/ref/*.md             ref 语料，每个主题一份
skills/bnx/SKILL.md      agent skill（bnx skills 原样打印）
tests/cli.rs             端到端测试（真实 daemon + worker + BN 6.0）
```

## 已知限制

- 阻塞调用无法取消（见上）。
- `ref` 是词面检索（名字命中远重于 docstring 命中），不懂同义词：没有叫 rename 的 API，这类写法靠语料里的笔记补。
- 没有 trace；不支持 Windows。

## 评测

CLI、skill、自带 MCP 与官方或第三方 MCP 在同一任务集上的对比见 [评测报告](../axkit/eval/reports/2026-10-24-slim.md)；任务集在 [`eval/`](eval)。

## 许可

Apache-2.0。
