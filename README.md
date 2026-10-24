# bnx

Binary Ninja 6.0 for agents: a persistent headless session scripted in native Python, as a CLI and a code-mode MCP server.

bnx 是面向 agent 的 Binary Ninja 命令行工具：会话常驻在后台 daemon 里，查询和修改都写成 Python 脚本，直接调用 Binary Ninja 原生的 Python API。

命令、会话、daemon 和输出格式都由 [rex](https://github.com/VibRev/rex) 提供，约定见 [rex CONVENTIONS](https://github.com/VibRev/rex/blob/main/docs/CONVENTIONS.md) 和 [axkit CONVENTIONS](https://github.com/VibRev/axkit/blob/main/docs/CONVENTIONS.md)。本仓库只放 Binary Ninja 自己的部分：worker 里的后端、Python console 与 prelude、`ref` 语料和帮助文本。

```sh
bnx exec fw.bin 'len(bv.functions)'
bnx exec fw.bin 'str(bv.get_functions_by_name("main")[0].hlil)'
bnx ref get_functions_at
bnx save fw.bin && bnx close fw.bin
```

## 获取与构建

bnx 通过 `path = "../rex"`、`path = "../axkit"` 依赖另外两个仓库，三个仓库要 clone 到同一个父目录下：

```sh
mkdir VibRev && cd VibRev
git clone https://github.com/VibRev/axkit
git clone https://github.com/VibRev/rex
git clone https://github.com/VibRev/bnx
cd bnx && BINARYNINJADIR=/path/to/binaryninja cargo build --release
```

- **只支持 Binary Ninja 6.0 R2**（`stable/6.0.10905`，API 提交 `775c3b5e`）。
  - `binaryninja` 和 `binaryninjacore-sys` 以 git 依赖固定在这个提交，与安装目录里 `api_REVISION.txt` 记录的一致。
  - worker 启动时会检查加载到的 core 版本，与构建时的目标不一致就报 `engine_error`。
- **`BINARYNINJADIR`** 指向放 core 动态库的目录。`build.rs` 链接 `libbinaryninjacore` 并在 Linux、macOS 上写入 rpath，运行时不需要设置 `LD_LIBRARY_PATH`（Windows 见下）。
- **license**：运行需要一份可以 headless 使用的 license，放在下面「平台」一节列出的位置，或用 `BN_LICENSE_FILE` 指定。`--help` 和 `ref` 不需要 license。

## 平台

- **Linux**：`BINARYNINJADIR` 设为安装目录。license 默认在 `~/.binaryninja/license.dat`。
- **macOS**（实测 macOS 15 / Apple Silicon）：
  - `BINARYNINJADIR=/Applications/BinaryNinja.app/Contents/MacOS`；
  - bindgen 用 Xcode 自带的 libclang；
  - Python API 和示例在 app 的 `Contents/Resources/` 下，`ref` 会自动找到；
  - license 默认在 `~/Library/Application Support/Binary Ninja/license.dat`。

- **Windows**（10 1803 及以上，x86_64；尚未在真机上验证）：
  - `BINARYNINJADIR` 设为安装目录（默认 `C:\Program Files\Vector35\BinaryNinja`，里面有 `binaryninjacore.dll`）；构建需要 MSVC 与 LLVM（bindgen 用 libclang）。
  - Windows 没有 rpath：运行时安装目录要在 `PATH` 上（`$env:PATH = "$env:BINARYNINJADIR;$env:PATH"`）。
  - license 默认在 `%APPDATA%\Binary Ninja\license.dat`。
  - skill 和示例里的命令是 bash 写法：PowerShell 里单引号同样不展开，照抄能用；代码里本身有单引号时改用 `-f FILE` / `-`（stdin）。`ref` 列出的 `grep` 在 PowerShell 里换成 `Select-String`。

daemon 用 Unix socket（Windows 上是原生 AF_UNIX）。

## 命令

命令和全局 flag 与 idax 完全相同，见 [rex README](https://github.com/VibRev/rex/blob/main/README.md) 里的「命令与全局 flag」一节。与 idax 的差别：

- 环境变量前缀是 `BNX_`。
- `save` 默认写 `<target>.bndb`；target 本身是 `.bndb` 时覆盖它自己。保存后输出 `saved | <path>`。

会话行的格式：

```
# target | analysis | idle
/work/fw.bin | complete | 0s
```

## exec 里有什么

```sh
bnx exec fw.bin - <<'EOF'
[{"addr": hex(f.start), "name": f.name} for f in bv.functions if "crypt" in f.name]
EOF
```

- **命名空间**：`bv` 是当前会话的 BinaryView，已经执行过 `from binaryninja import *`。没有额外的 helper；不确定 API 时用 `bnx ref <words>` 查。每次 exec 的全局变量都是新的。
- **结果**：取最后一个表达式的值。不能转成 JSON 的对象输出它的 `str()`。
- **异常**：traceback 只保留用户代码的帧；`AttributeError`、`NameError` 会给出近似的名字（对 `bv.` 和模块属性同样有效）。
- **取消**：客户端断开时，daemon 发出取消，worker 调用 `BNCancelScriptInput`，正在执行的 Python 代码收到 `KeyboardInterrupt`，会话保留。
  - **阻塞在 C 里的调用打断不了**，比如 `time.sleep` 或耗时很长的 core 调用：console 不在主线程上跑 Python，信号送不到。
  - 这种情况下等 10 秒宽限期，然后 worker 被杀，会话丢失。

## ref：在 Binary Ninja 安装里查 API

`ref` 语料只写 bnx 自己的笔记，分三个主题：`exec`（结果语义、脚本、错误）、`session`、`pitfalls`（改名、headless 下的坑）。Binary Ninja 的 API 参考和官方示例不打包，而是在运行时实际加载的那份安装里现查。

**`bnx ref <words>`**
- 逐行解析 `<BNDIR>/python/binaryninja/**/*.py`，约 5700 个定义，一次约 25 ms，不缓存。
- 每条命中给出 `限定名(签名) -> 返回 | 绝对路径:行号 | docstring 首行`；名字只有一处定义或精确命中时，附完整 docstring。
- 最后附上相关的官方示例。
- 查询是标识符（含 `_`、`.` 或大写字母）时，API 结果排在前面；普通词（如 `rename`）先给笔记。

```
$ bnx ref get_functions_at
binaryninja.BinaryView.get_functions_at(addr: int) -> List['_function.Function'] | <BNDIR>/python/binaryninja/binaryview.py:6076
    get_functions_at get a list of Function objects (one for each valid platform) that start at the
    ...
$ bnx ref rename
# ref pitfalls: rename a function
Assign the name: func.name = "new_name" (there is no rename call); ...
```

**不带参数的 `bnx ref`**：先列出主题，再列出安装里的位置和可以直接粘贴的 grep 命令：
- `<BNDIR>/python/binaryninja`：签名和 docstring；
- `<BNDIR>/examples/python`：官方示例。

**检索的局限**：按词面匹配，不懂同义词。Binary Ninja 里没有叫 rename 的 API，这类写法靠笔记补上。

## MCP

`bnx mcp` 是 stdio MCP server，采用 code mode，只有五个工具，每个都对应同名命令：

- `exec {target, code, timeout?}`
- `ref {query}`
- `save {target, path?, force?}`
- `close {target}`
- `list {}`

工具定义合计约 2 KB。和 CLI 的关系：

- **共用 daemon 和会话**：在 MCP 里打开的 target，命令行 `bnx status` 也能看到。
- **输出同样有上限**：和 agent 模式的 CLI 一样约 8 KB，超出部分写进 spill 文件，最后一行给出路径。
- **报错格式相同**：`Error:` 加 `hint:`，工具结果标 `isError`。

配置方法：

```sh
claude mcp add bnx -- bnx mcp       # Claude Code
```

项目里的 `.mcp.json`：

```json
{"mcpServers": {"bnx": {"command": "bnx", "args": ["mcp"]}}}
```

codex：

```toml
# ~/.codex/config.toml
[mcp_servers.bnx]
command = "bnx"
args = ["mcp"]
```

## skill

`skills/bnx/SKILL.md` 是给 agent 用的 skill，打包进二进制。bnx 只负责把它打印出来，不负责安装：各家 harness 的 skill 目录不同，而且一直在变，由 agent 按自己所在的 harness 安装最可靠。

```sh
bnx skills                 # 原样打印 SKILL.md，不截断
bnx skills list            # skill 的全部文件
bnx skills show SKILL.md   # 其中一个文件
```

frontmatter 的 `metadata.version` 与 `bnx --version` 相同。agent 拿已装副本的版本和它比，就知道是否过期。

## 后端

**每个会话一个 `bnx __worker` 子进程**，由 rex 管理；daemon 本身不初始化 core。这样做有四个原因：

1. **崩溃隔离**：core 或脚本崩溃（segfault、`os._exit`、OOM）只会带走一个会话。
2. **超时必须能杀**：卡在 core 调用里的 Python 无法中断，只能杀进程。
3. **Python 解释器是进程级的全局状态**：一个进程只开一个 view，会话之间不共享 `sys.modules` 和全局变量。
4. **不碰 core 就不需要 license**：daemon 不初始化 core，所以 `ref`、`--help`、`list` 都不需要 license。

**打开与保存**
- 打开 `.bndb` 时，rex 先把它复制进会话的工作目录，再打开副本。
- `save` 每次都先 `create_database` 到目标旁边的临时路径，再由 rex rename 到位。

**脚本怎么交给 console**
- scripting provider 的 console 按 REPL 的方式编译输入，所以只给它一行调用 `__bnx_run(<请求文件>)`；脚本和结果都走工作目录里的 JSON 文件，结果文件经 rename 出现就表示执行完成。
- 每次提交前都等 console 进入 `ReadyForScriptExecution`。机器繁忙、解释器启动慢，或者上一次提交还没完全返回时，提前提交的输入会被当成无效输入拒绝。

## 开发

```sh
cargo clippy --all-targets
cargo test        # 端到端测试需要 license，没有时自动跳过
cargo fmt --check
```

- **fixture** 与 idax 相同：用 rex 的 `fixtures/fw.bin`，没有交叉编译器时用 `rex/fixtures/bin` 里的预编译产物。
- **示例测试**：`tests/cli.rs` 的 `every_skill_example_runs` 和 `every_ref_example_runs` 会逐条执行 SKILL.md 和 `ref` 语料里的示例。
- **评测**：任务集在 [`eval/`](https://github.com/VibRev/bnx/tree/main/eval)；评测结论见 [评测报告](https://github.com/VibRev/axkit/blob/main/eval/reports/2026-10-24-slim.md)。

源码目录：

```
src/main.rs              Tool 描述（名字、帮助、语料），入口 rex::tool::main
src/backend/mod.rs       rex::worker::Backend：headless 初始化、版本检查、加载、分析等待、保存
src/backend/console.rs   python3 scripting provider 的 C API
src/backend/prelude.py   Python prelude
src/ref/*.md             ref 语料，每个主题一份
skills/bnx/SKILL.md      agent skill
tests/cli.rs             端到端测试（真实的 daemon + worker + BN 6.0）
```

## 已知限制

- 阻塞在 C 里的调用无法取消（见上文「取消」）。
- 没有 trace。
- Windows 尚未在真机上验证（只做了交叉编译和 CI），遇到问题请提 issue。

## 许可

Apache-2.0。
