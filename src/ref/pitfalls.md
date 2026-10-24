# pitfalls: the Binary Ninja API headless
## a change starts analysis again
Renaming, retyping or creating a function queues analysis; call bv.update_analysis_and_wait() in the same script before reading what depends on it.
    bnx exec fw.bin 'f = bv.get_functions_by_name("check")[0]; f.type = "int32_t check(char const* s)"; bv.update_analysis_and_wait(); str(f.type)'
## rename a function
Assign the name: func.name = "new_name" (there is no rename call); a data variable is named with bv.define_user_symbol. The example puts the old name back.
    bnx exec fw.bin 'f = bv.get_functions_by_name("check")[0]; f.name = "check_pw"; found = [g.start == f.start for g in bv.get_functions_by_name("check_pw")]; f.name = "check"; found'
## Ctrl-C and blocking calls
Pure Python is interrupted with KeyboardInterrupt and the session stays; a script blocked inside Binary Ninja or a C call (time.sleep) cannot be, and its worker is killed after 10 s.
## the engine's own reference
`bnx ref <words>` searches the installed binaryninja package; `bnx ref` with no words lists where the package and the official examples are.
    bnx ref
