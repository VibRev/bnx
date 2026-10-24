# bnx 评测任务集

这是给 [axkit-eval](https://github.com/VibRev/axkit/tree/main/eval) 用的 bnx 任务集。它和 [idax/eval](https://github.com/VibRev/idax/tree/main/eval) 用同一批 ARM64 fixture、同一批问题，只有工具名、环境变量和落盘数据库的扩展名（`.bndb`）不同，方便把两个引擎放在一起比。

**结论**：CLI、CLI + skill 和 `bnx mcp` 都通过了全部任务。其中 `bnx mcp` 在 claude 上每个会话约 31k token，是 bn-headless-mcp 的 1/4 左右。详见 [评测报告](https://github.com/VibRev/axkit/blob/main/eval/reports/2026-10-24-slim.md)。

## 跑

```sh
BINARYNINJADIR=/path/to/binaryninja cargo build   # 先有 target/debug/bnx
cd eval
axkit-eval check suite.toml
axkit-eval run suite.toml --group claude --task password --reps 1
axkit-eval run suite.toml
axkit-eval summary ../target/eval-data/results.jsonl
```

`setup` 和 idax 一样，用 `aarch64-linux-gnu-gcc` 编译 `fixtures/*.c`，真值也相同。

- 没有交叉编译器的机器（例如 macOS）会从 `fixtures/bin/` 拷贝预编译产物。
- bnx 能分析任意架构，这里沿用 ARM64，只是为了和 idax 对齐。

## 任务与分组

- **任务**：与 [idax/eval](https://github.com/VibRev/idax/blob/main/eval/README.md) 相同，共 7 个：`password`、`xor-flag`、`add-cipher`、`calls-system`、`rename-save`、`spill-needle`、`long-function`。
- **分组**：
  - `claude` / `codex` / `grok`：CLI，不带 skill；
  - `*-skill`：CLI，并把 skill 装成项目级 skill；
  - `*-mcp`：`bnx mcp`；
  - `*-bhmcp`：bn-headless-mcp，位置由 `AXEVAL_BN_HEADLESS_MCP` 给出；
  - `smoke-*`：冒烟。
- **claude 的权限**：claude 组用 `--permission-mode auto` 运行。
- **隔离**：每次运行有独立的 `BNX_RUNTIME_DIR` 和 `XDG_CACHE_HOME`。
- **`grok-bhmcp` 实际拿不到任何工具**：grok 会跳过名字里带 `.` 的 MCP 工具，而 bn-headless-mcp 的工具名都带 `.`。

## 判定脚本

`fixtures/check_rename.sh` 在工作区里找保存出来的 `.bndb`，不限于 target 旁边，逐个用 bnx 重新打开；只要有一个的函数名里出现 `decrypt_me`，就算通过。
