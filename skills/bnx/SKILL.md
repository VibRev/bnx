---
name: bnx
description: Reverse engineer native binaries (ELF, PE, Mach-O, firmware blobs) with Binary Ninja from the shell through the bnx CLI - decompile to HLIL and disassemble functions, list functions, strings and imports, follow cross references, rename, comment, retype and patch, then save a .bndb. Use when analysing or changing a native binary with Binary Ninja, or answering "what does this function do", "who calls X", "where is this string used".
metadata:
  version: "0.2.0"
---

# bnx

`bnx exec <target> 'CODE'` runs Binary Ninja Python against the target and prints the
value of the last expression (after anything printed). The first command opens the target
in a background daemon and waits for analysis; later commands reuse it. `bv` is the
BinaryView and everything from `binaryninja` is imported.

Unsure of an API? `bnx ref <words>` (e.g. `bnx ref code refs`) searches the installed
`binaryninja` package and prints signatures with file:line. Do not guess.

The same sessions are also served over MCP: `bnx mcp` is a stdio server with the tools
exec, ref, save, close and list. Configure it in the MCP client when shell access is not available.

## Rules

1. Read before you change; read back after you change.
2. Nothing is written until `bnx save <target>`. `close` drops unsaved changes.
3. Close the sessions you opened: `bnx close <target>`.
4. Write the code inside the exec quotes, several lines if needed.

## Examples

```sh
bnx exec fw.bin '[(hex(f.start), f.name, f.total_bytes) for f in bv.functions]'
bnx exec fw.bin 'str(bv.get_functions_by_name("main")[0].hlil)'
bnx exec fw.bin '[c.name for c in bv.get_functions_by_name("puts")[0].callers]'
bnx exec fw.bin 'a = bv.get_symbol_by_raw_name("tries").address
sorted({r.function.name for r in bv.get_code_refs(a)})'
bnx exec fw.bin '[(hex(s.start), s.value) for s in bv.strings if "access" in s.value]'
bnx exec fw.bin 'f = bv.get_functions_by_name("check")[0]
f.name = "check_pw"
f.name'
bnx save fw.bin
bnx close fw.bin
```

A binary saves as `fw.bin.bndb` beside it, a `.bndb` over itself, `--to PATH` elsewhere.

## Output

Under an agent the output is cut at about 8 KB; the last line then names a file with all of
it. Errors print `Error: <kind>: <message>` and a `hint:` line. Exit 3 means the session was
lost (rerun). `bnx --help` and `bnx ref` have the rest.
