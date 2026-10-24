# session: sessions, analysis and saving
## sessions
The first command on a target opens it in a background daemon and waits for analysis; later commands reuse it. `./fw.bin`, `/abs/fw.bin` and a symlink are one session.
    bnx list
    bnx status fw.bin
## saving
Nothing touches the disk until `save`. A binary saves as `<name>.bndb` beside it, a .bndb over itself; `--to PATH` writes elsewhere. `close` never saves.
    bnx save fw.bin
    bnx save fw.bin --to fw-annotated.bndb
## lost sessions
Exit 3 means the worker crashed or timed out: the session and its unsaved changes are gone. Rerun to open a fresh one; `--timeout SECS` gives slow scripts longer (0: no limit).
## as an MCP server
`bnx mcp` is a stdio MCP server with five tools, exec, ref, save, close and list, each one the command of that name; configure the command `bnx` with the argument `mcp` in the MCP client. Sessions, output and saving work as on the command line.
