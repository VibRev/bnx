# bnx 评测任务集

给 [axkit-eval](../../axkit/eval) 用的 bnx 任务集，与 [idax/eval](../../idax/eval) 用同一批 ARM64 fixture、同一批问题，只有工具名、环境变量和落盘数据库扩展名（`.bndb`）不同，便于两个引擎并排读。

## 跑

```sh
BINARYNINJADIR=/path/to/binaryninja cargo build   # 先有 target/debug/bnx
cd eval
axkit-eval check suite.toml
axkit-eval run suite.toml --group noskill --task password --reps 1
axkit-eval run suite.toml
axkit-eval summary ../target/eval-data/results.jsonl
```

`setup` 同样用 `aarch64-linux-gnu-gcc` 编 `fixtures/*.c`；fixture 与 idax 一致，真值相同。没有交叉编译器的主机（macOS）从 `fixtures/bin/` 拷预编译产物，那里存着编译用的源码，源码变了就拒绝并提示在有编译器的机器上 `make -C fixtures prebuilt`。bnx 能分析任意架构，这里沿用 ARM64 只为和 idax 对齐。

## 任务与分组

任务表与 [idax/eval](../../idax/eval/README.md) 相同（`password` / `xor-flag` / `add-cipher` / `calls-system` / `rename-save` / `spill-needle` / `long-function`），fixture 是同一份二进制。claude 组同样以 `--permission-mode auto` 运行，不设允许列表。分组 `noskill` 与 `skill`（装 `../skills/bnx`）。每个运行有独立的 `BNX_RUNTIME_DIR` 和 `XDG_CACHE_HOME`。

CLI 对 MCP：`claude-bhmcp` / `codex-bhmcp` / `grok-bhmcp` 换成 bn-headless-mcp，位置由运行器环境的 `AXEVAL_BN_HEADLESS_MCP` 给出。grok 会跳过名字含 `.` 的 MCP 工具，而 bn-headless-mcp 的工具名都带 `.`，所以 `grok-bhmcp` 实际拿不到任何工具。

`fixtures/check_rename.sh` 在工作区里找保存出来的 `.bndb`（不限于 target 旁边），逐个用 bnx 重开，任一个的函数名里出现 `decrypt_me` 即通过。

- `bnx` 自己的 MCP：`claude-mcp` / `codex-mcp` / `grok-mcp` 用 `bnx mcp`（stdio，code mode：`exec`、`ref`、`save`、`close`、`list`）代替 CLI，其余设置与上面的 MCP 组相同：PATH 上没有 CLI，不装 skill，claude 和 grok 没有 shell，codex 的 shell 只读。
