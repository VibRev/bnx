# exec: what a script sees
## the namespace and the result
bv is the session's BinaryView and everything from `binaryninja` is imported. Every exec starts with fresh globals. The result is the value of the last expression; print() lines come first.
    bnx exec fw.bin 'bv.view_type, bv.arch.name, len(bv.functions)'
## what converts to JSON
dict/list/tuple/str/int/float/bool/None as they are; bytes as hex; iterators are drained; anything else (a Function, a Type) becomes its str(). A list of dicts prints as a table.
    bnx exec fw.bin '[{"addr": hex(f.start), "name": f.name} for f in bv.functions][:3]'
## code, files and stdin
The code is the argument after the target: one shell argument, as many lines as it needs. `-c CODE` says the same, and several `-c` run as one script. `-f FILE` runs a file; `-` or `-f -` reads the code from stdin.
    bnx exec fw.bin 'for f in bv.functions:
        print(hex(f.start), f.name)'
    bnx exec fw.bin - <<'EOF'
    print(len(bv.functions))
    EOF
## errors
An exception exits 1 with `Error: script_error: <Type>: <message> (at <exec>:<line>)`; the traceback, cut to your code, is on stderr. A misspelt name gets `hint: did you mean: ...?`.
    bnx exec fw.bin 'bv.functons'   # exit 1
