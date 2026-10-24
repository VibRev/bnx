# bnx prelude: installed once into the worker's Python console.
#
# The console only ever receives one-line calls (`__bnx_run(<request path>)`);
# the request and the answer travel as JSON files, so nothing has to be parsed
# out of the console's fragmented output stream. The answer is written to a
# temporary name and renamed, so its appearance means it is complete.


def __bnx_install(bv):
    import ast
    import builtins
    import contextlib
    import difflib
    import io
    import json
    import linecache
    import os
    import sys
    import traceback

    import binaryninja

    def fresh():
        ns = {"__name__": "__main__", "__builtins__": builtins}
        exec("from binaryninja import *", ns)
        ns["bv"] = bv
        ns["binaryninja"] = binaryninja
        return ns

    def to_json(value, depth=0):
        if value is None or isinstance(value, (bool, int, float, str)):
            return value
        if depth > 32:
            return repr(value)
        if isinstance(value, dict):
            return {str(k): to_json(v, depth + 1) for k, v in value.items()}
        if isinstance(value, (list, tuple, set, frozenset)) or _is_iterator(value):
            return [to_json(v, depth + 1) for v in value]
        if isinstance(value, (bytes, bytearray)):
            return value.hex()
        return str(value)

    def _is_iterator(value):
        return hasattr(value, "__next__") or type(value).__name__ in ("generator", "map", "filter", "zip")

    def suggestions(e, ns):
        name = getattr(e, "name", None)
        if not isinstance(name, str):
            return []
        if isinstance(e, AttributeError):
            obj = getattr(e, "obj", None)
            pool = [n for n in dir(obj) if not n.startswith("__")] if obj is not None else []
            return difflib.get_close_matches(name, pool, n=3, cutoff=0.6)
        if not isinstance(e, NameError):
            return []
        # A bare name is often the view's or a module's: get_code_refs is bv.get_code_refs.
        owners = {"bv": bv, **{m: mod for m, mod in sys.modules.items() if m.startswith("binaryninja.") and mod is not None}}
        exact = [f"{o}.{name}" for o, obj in owners.items() if hasattr(obj, name)]
        attrs = {f"bv.{a}": a for a in dir(bv) if not a.startswith("_")}
        close = set(difflib.get_close_matches(name, attrs.values(), n=3, cutoff=0.8))
        near = [q for q, a in attrs.items() if a in close and q not in exact]
        local = difflib.get_close_matches(name, list(ns) + dir(builtins), n=3, cutoff=0.6)
        return [*exact, *local, *near][:5]

    def failure(e, ns, filename):
        tb = traceback.extract_tb(e.__traceback__)
        user = [fr for fr in tb if fr.filename == filename]
        location = f"{filename}:{user[-1].lineno}" if user else None
        if isinstance(e, SyntaxError) and e.filename == filename:
            location = f"{filename}:{e.lineno}"
        return {
            "kind": type(e).__name__,
            "message": str(e) if not isinstance(e, SyntaxError) else (e.msg or str(e)),
            "location": location,
            "suggestions": suggestions(e, ns),
            "traceback": "".join(traceback.format_list(user)),
        }

    def run(request_path):
        with open(request_path, encoding="utf-8") as fh:
            req = json.load(fh)
        filename = req["filename"]
        ns = fresh()
        out, err = io.StringIO(), io.StringIO()
        answer = {"stdout": "", "stderr": "", "result": None, "error": None}
        old_argv = sys.argv
        sys.argv = [filename]
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                try:
                    # So tracebacks can quote the user's lines.
                    linecache.cache[filename] = (len(req["code"]), None, req["code"].splitlines(True), filename)
                    tree = ast.parse(req["code"], filename=filename)
                    last = None
                    if tree.body and isinstance(tree.body[-1], ast.Expr):
                        last = ast.Expression(tree.body.pop().value)
                    exec(compile(tree, filename, "exec"), ns)
                    value = eval(compile(last, filename, "eval"), ns) if last is not None else None
                    answer["result"] = to_json(value)
                except BaseException as e:  # noqa: BLE001 - every failure is the script's answer
                    answer["error"] = failure(e, ns, filename)
        finally:
            sys.argv = old_argv
        if answer["error"] is not None and answer["error"]["traceback"]:
            err.write("Traceback (most recent call last, user code only):\n" + answer["error"]["traceback"])
        answer["stdout"], answer["stderr"] = out.getvalue(), err.getvalue()
        if answer["error"] is not None:
            del answer["error"]["traceback"]
        tmp = req["answer"] + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(answer, fh)
        os.replace(tmp, req["answer"])

    return run


def __bnx_ready(path):
    import os

    with open(path + ".tmp", "w") as fh:
        fh.write("ok")
    os.replace(path + ".tmp", path)
