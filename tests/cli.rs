//! bnx end to end on a copy of `/usr/bin/true`: a real daemon, a real
//! worker, Binary Ninja 6.0. Needs the license the build machine uses; the
//! tests skip themselves without one.

// Test code: a failed unwrap is the test failing, with its message.
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

struct Env {
    /// Holds the target copy, the cache and the socket directory.
    dir: tempfile::TempDir,
    target: PathBuf,
}

impl Env {
    fn new() -> Option<Env> {
        let licensed = std::env::var_os("BN_LICENSE_FILE").is_some()
            || std::env::var_os("HOME").is_some_and(|h| {
                [".binaryninja/license.dat", "Library/Application Support/Binary Ninja/license.dat"]
                    .iter()
                    .any(|f| Path::new(&h).join(f).is_file())
            });
        if !licensed {
            eprintln!("skipped: no Binary Ninja license");
            return None;
        }
        // Under /tmp: a socket path has to stay short.
        let dir = tempfile::Builder::new().prefix("bnx-it").tempdir_in("/tmp").unwrap();
        for sub in ["rt", "cache", "w"] {
            std::fs::create_dir(dir.path().join(sub)).unwrap();
            std::fs::set_permissions(dir.path().join(sub), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        // Canonical: on macOS /tmp is a symlink to /private/tmp, and sessions
        // are keyed by the canonical path.
        let target = dir.path().canonicalize().unwrap().join("w/true");
        std::fs::copy(small_elf(), &target).unwrap();
        Some(Env { dir, target })
    }

    fn bnx(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.command(args, env).output().unwrap()
    }

    fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bnx"));
        cmd.args(args)
            .current_dir(self.dir.path())
            .env("BNX_RUNTIME_DIR", self.dir.path().join("rt"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env_remove("CLAUDECODE")
            .env_remove("AXKIT_AGENT")
            .envs(env.iter().copied());
        cmd
    }

    /// `script` in `sh`, with the environment `command` gives bnx.
    fn shell(&self, script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(script)
            .current_dir(self.dir.path())
            .env("BNX_RUNTIME_DIR", self.dir.path().join("rt"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env_remove("CLAUDECODE")
            .env_remove("AXKIT_AGENT");
        cmd
    }

    /// rex's shared fixture as `fw.bin`: AArch64 like idax's, so the two
    /// answer about the same binary; the host compiler without a cross one.
    fn fixture(&self) -> bool {
        aarch64_fixture("fw.c", "fw.bin", &self.dir.path().join("fw.bin"))
    }

    fn target(&self) -> &str {
        self.target.to_str().unwrap()
    }
}

/// The binary the session tests call `true`: the host's `/usr/bin/true`
/// where that is an ELF (Linux), else rex's prebuilt AArch64 `fw.bin`. A
/// macOS `true` is a one-function Mach-O, too small to exercise truncation.
fn small_elf() -> PathBuf {
    let host = Path::new("/usr/bin/true");
    if fs::read(host).ok().is_some_and(|b| b.starts_with(b"\x7fELF")) {
        host.to_path_buf()
    } else {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../rex/fixtures/bin/fw.bin")).to_path_buf()
    }
}

/// rex's AArch64 fixture `src` (`fw.c`, `demo.c`) built as `out`: by the
/// cross compiler where there is one, else copied from `rex/fixtures/bin`
/// (macOS hosts) as long as that copy was built from the same source.
fn aarch64_fixture(src: &str, prebuilt: &str, out: &Path) -> bool {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../rex/fixtures"));
    let built = Command::new("aarch64-linux-gnu-gcc").arg("-O1").arg("-o").arg(out).arg(dir.join(src)).status();
    if built.is_ok_and(|s| s.success()) {
        return true;
    }
    let fresh = fs::read(dir.join(src)).ok().is_some_and(|now| fs::read(dir.join("bin").join(src)).ok().as_ref() == Some(&now));
    if !fresh {
        eprintln!("no aarch64-linux-gnu-gcc, and rex/fixtures/bin/{src} is stale: run `make -C rex/fixtures prebuilt`");
        return false;
    }
    fs::copy(dir.join("bin").join(prebuilt), out).is_ok()
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.bnx(&["daemon", "stop", "--force"], &[]);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn a_session_from_open_to_close() {
    let Some(env) = Env::new() else { return };
    let t = env.target();

    let open = env.bnx(&["open", t], &[]);
    assert_eq!(open.status.code(), Some(0), "{}", text(&open.stderr));
    let out = text(&open.stdout);
    assert!(out.starts_with("# target | analysis | idle\n"), "{out}");
    assert!(out.contains(&format!("{t} | complete | ")), "{out}");

    // An agent's default window cuts the function table; the hint names the
    // full output, which is exactly what --full prints.
    let cut = env.bnx(&["exec", t, "-c", FUNCTIONS], &[("CLAUDECODE", "1"), ("BNX_MAX_BYTES", "200")]);
    assert_eq!(cut.status.code(), Some(0));
    let out = text(&cut.stdout);
    assert!(out.starts_with("# addr | name | size\n0x"), "{out}");
    let hint = out.lines().last().unwrap();
    let rest = hint.strip_prefix("# records 1-").unwrap();
    let path = rest.split_once("; full output saved to ").unwrap().1;
    assert!(!hint.contains("--offset"), "{hint}");
    let full = env.bnx(&["exec", t, "--full", "-c", FUNCTIONS], &[("CLAUDECODE", "1")]);
    assert_eq!(std::fs::read_to_string(path).unwrap(), text(&full.stdout));

    // Prints come first, then the last expression's value.
    let script = env.bnx(&["exec", t, "-c", "print('first')\n{'entry': hex(bv.entry_point)}"], &[]);
    let out = text(&script.stdout);
    assert!(out.starts_with("first\nentry | 0x"), "{out}");
    // A value that is not JSON prints as its str(), without a note.
    let object = env.bnx(&["exec", t, "-c", "bv.get_functions_by_name('main')[0].type"], &[]);
    assert_eq!(text(&object.stdout), "int32_t(int32_t argc, char** argv, char** envp)\n", "{}", text(&object.stderr));
    assert!(object.stderr.is_empty(), "{}", text(&object.stderr));
    // Every exec starts with fresh globals.
    env.bnx(&["exec", t, "-c", "leftover = 1"], &[]);
    let fresh = env.bnx(&["exec", t, "-c", "leftover"], &[]);
    assert_eq!(fresh.status.code(), Some(1), "{}", text(&fresh.stdout));

    // Every spelling of the target is the same session (rex R-01).
    let link = env.dir.path().join("link");
    std::os::unix::fs::symlink(&env.target, &link).unwrap();
    let status = env.bnx(&["status", "link"], &[]);
    assert_eq!(status.status.code(), Some(0), "{}", text(&status.stderr));
    assert!(text(&status.stdout).contains(&format!("\n{t} | complete | ")), "{}", text(&status.stdout));

    // An unsaved change is gone after close (rex R-03).
    let rename = env.bnx(&["exec", t, "-c", "bv.get_functions_by_name('main')[0].name = 'renamed'"], &[]);
    assert_eq!(rename.status.code(), Some(0), "{}", text(&rename.stderr));
    let close = env.bnx(&["close", t], &[]);
    assert_eq!(close.status.code(), Some(0));
    assert_eq!(text(&close.stdout), format!("closed | {t}\n"));
    assert!(!Path::new(&format!("{t}.bndb")).exists());
    let gone = env.bnx(&["status", t], &[]);
    assert_eq!(gone.status.code(), Some(1));
    assert_eq!(text(&gone.stderr), format!("Error: not_found: no session for {t}\nhint: bnx open {t}\n"));
    let reopened = env.bnx(&["exec", t, "-c", "bool(bv.get_functions_by_name('main')) and not bv.get_functions_by_name('renamed')"], &[]);
    assert_eq!(text(&reopened.stdout), "true\n", "{}", text(&reopened.stderr));

    // Saved changes are in the database.
    env.bnx(&["exec", t, "-c", "bv.get_functions_by_name('main')[0].name = 'kept'"], &[]);
    let save = env.bnx(&["save", t], &[]);
    assert_eq!(text(&save.stdout), format!("saved | {t}.bndb\n"));
    let db = format!("{t}.bndb");
    let from_db = env.bnx(&["exec", &db, "-c", "bool(bv.get_functions_by_name('kept'))"], &[]);
    assert_eq!(text(&from_db.stdout), "true\n", "{}", text(&from_db.stderr));

    // A database target saves over itself, and it is its own file.
    env.bnx(&["exec", &db, "-c", "bv.get_functions_by_name('kept')[0].name = 'kept_again'"], &[]);
    assert_eq!(text(&env.bnx(&["save", &db], &[]).stdout), format!("saved | {db}\n"));
    env.bnx(&["close", &db], &[]);
    let again = env.bnx(&["exec", &db, "-c", "bool(bv.get_functions_by_name('kept_again'))"], &[]);
    assert_eq!(text(&again.stdout), "true\n", "{}", text(&again.stderr));
    let leftovers: Vec<_> = std::fs::read_dir(env.dir.path().join("w")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(leftovers.len(), 2, "the target and its database, no partial files: {leftovers:?}");
}

#[test]
fn a_script_on_stdin_and_an_interrupted_client() {
    let Some(env) = Env::new() else { return };
    let t = env.target();
    for source in [&["-"][..], &["-f", "-"]] {
        let args: Vec<&str> = ["exec", t].into_iter().chain(source.iter().copied()).collect();
        let mut child = env.command(&args, &[]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(b"import sys\nprint(sys.argv)\nlen(bv.functions) > 0\n").unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(text(&out.stdout), "['<stdin>']\ntrue\n", "{source:?}");
    }
    let multi = env.bnx(&["exec", t, "n = 0\nfor f in bv.functions:\n    n += 1\nn > 0"], &[]);
    assert_eq!(text(&multi.stdout), "true\n", "{}", text(&multi.stderr));
    assert_eq!(text(&env.bnx(&["exec", t, "-c", "a = 2", "-c", "a * 3"], &[]).stdout), "6\n");
    assert_eq!(text(&env.bnx(&["exec", t, "[f for f in bv.functions if f.start < 0]"], &[]).stdout), "[]\n");

    // Running Python is interrupted; a call blocked in C (time.sleep) is not,
    // and its worker would be killed after the grace period.
    let mut client = env.command(&["exec", t, "-c", "while True: pass"], &[]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    client.kill().unwrap();
    client.wait().unwrap();
    let after = env.bnx(&["exec", t, "-c", "1 + 1"], &[]);
    assert_eq!(text(&after.stdout), "2\n", "{}", text(&after.stderr));
    assert!(!text(&after.stderr).contains("# progress: opening"), "the session survived: {}", text(&after.stderr));
}

#[test]
fn errors_say_what_to_do_and_exit_by_kind() {
    let Some(env) = Env::new() else { return };
    let t = env.target();

    let attr = env.bnx(&["exec", t, "-c", "bv.functons"], &[]);
    assert_eq!(attr.status.code(), Some(1));
    let err = text(&attr.stderr);
    assert!(err.contains("Error: script_error: AttributeError: 'BinaryView' object has no attribute 'functons' (at <exec>:1)\nhint: did you mean: functions"), "{err}");
    assert!(attr.stdout.is_empty());

    let json = env.bnx(&["exec", t, "--json", "-c", "missing_name"], &[]);
    assert_eq!(json.status.code(), Some(1));
    let last = text(&json.stderr).lines().last().unwrap().to_owned();
    let v: Value = serde_json::from_str(&last).unwrap();
    assert_eq!(v["error"]["kind"], "script_error");
    assert_eq!(
        serde_json::from_slice::<Value>(&json.stdout).unwrap(),
        serde_json::json!({"stdout": "", "result": null, "analysis": {"complete": true, "progress": 1.0}})
    );

    let usage = env.bnx(&["exec", t, "--limt", "1", "-c", "1"], &[]);
    assert_eq!(usage.status.code(), Some(1), "a usage error is 1, not clap's 2");
    assert!(text(&usage.stderr).starts_with("Error: usage: unexpected argument '--limt' found\nhint: "), "{}", text(&usage.stderr));
    assert!(text(&usage.stderr).trim_end().ends_with("`bnx exec --help`"), "{}", text(&usage.stderr));

    let missing = env.bnx(&["open", "no/such/file"], &[]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(text(&missing.stderr).starts_with("Error: not_found: "), "{}", text(&missing.stderr));

    let timeout = env.bnx(&["exec", t, "--timeout", "1", "-c", "import time; time.sleep(30)"], &[]);
    assert_eq!(timeout.status.code(), Some(3), "{}", text(&timeout.stderr));
    assert!(text(&timeout.stderr).starts_with("Error: timeout: "));

    let crash = env.bnx(&["exec", t, "-c", "import os; os.kill(os.getpid(), 9)"], &[]);
    assert_eq!(crash.status.code(), Some(3), "{}", text(&crash.stderr));
    assert!(text(&crash.stderr).contains("Error: session_lost: "), "{}", text(&crash.stderr));
    let after = env.bnx(&["exec", t, "-c", "1 + 1"], &[]);
    assert_eq!(text(&after.stdout), "2\n", "a lost session reopens on demand");
}

#[test]
fn the_daemon_and_an_in_process_run_print_the_same() {
    let Some(env) = Env::new() else { return };
    let t = env.target();
    for args in [&["exec", t, "--max-bytes", "120", "-c", FUNCTIONS][..], &["exec", t, "-c", "nme"], &["ref", "exec"]] {
        // Progress lines aside (AX C-12): only the local run opens afresh.
        let quiet =
            |o: &Output| text(&o.stderr).lines().filter(|l| !l.starts_with("# progress: ")).map(|l| format!("{l}\n")).collect::<String>();
        let daemon = env.bnx(args, &[]);
        let local = env.bnx(args, &[("BNX_DAEMON", "off")]);
        assert_eq!(
            (daemon.status.code(), text(&daemon.stdout), quiet(&daemon)),
            (local.status.code(), text(&local.stdout), quiet(&local)),
            "{args:?}"
        );
    }
}

/// `examples` from `markdown` run against the shared fixture as `fw.bin`
/// in `sh`, each ending with the exit status it says.
fn run_examples(env: &Env, markdown: &str, what: &str) -> usize {
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_bnx")).parent().unwrap().to_owned();
    let path = std::env::join_paths(std::iter::once(bin_dir).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))).unwrap();
    let examples = rex::example::examples(markdown, "bnx", "fw.bin");
    for example in &examples {
        let out = env.shell(&example.script).env("PATH", &path).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(i32::from(example.exit)),
            "{what} line {}: {}\n{}{}",
            example.line,
            example.script,
            text(&out.stderr),
            text(&out.stdout)
        );
    }
    examples.len()
}

#[test]
fn every_ref_example_runs() {
    let Some(env) = Env::new() else { return };
    if !env.fixture() {
        return;
    }
    let ran: usize = BNX_CORPUS.iter().map(|(name, markdown)| run_examples(&env, markdown, name)).sum();
    assert!(ran > 8, "only {ran} examples");
}

#[test]
fn every_skill_example_runs() {
    let Some(env) = Env::new() else { return };
    if !env.fixture() {
        return;
    }
    let ran = run_examples(&env, include_str!("../skills/bnx/SKILL.md"), "SKILL.md");
    assert!(ran > 6, "only {ran} examples");
    assert!(env.dir.path().join("fw.bin.bndb").exists(), "the skill's save example saved");
}

#[test]
fn ref_points_at_the_binary_ninja_install_and_its_greps_find_something() {
    let Some(env) = Env::new() else { return };
    let plain = env.bnx(&["ref"], &[]);
    assert!(plain.status.success(), "{}", text(&plain.stderr));
    assert!(text(&plain.stdout).contains("# path | holds | try\n"), "{}", text(&plain.stdout));
    let out = text(&plain.stdout);
    let install = out.lines().find_map(|l| l.strip_prefix("# the Binary Ninja API").and_then(|l| l.rsplit_once(": "))).unwrap().1;
    assert!(["libbinaryninjacore.so.1", "libbinaryninjacore.dylib"].iter().any(|lib| Path::new(install).join(lib).exists()), "{out}");
    let tries: Vec<&str> =
        out.lines().skip_while(|l| *l != "# path | holds | try").skip(1).filter_map(|l| l.rsplit(" | ").next()).collect();
    assert!(tries.len() >= 2, "{out}");
    for try_ in tries {
        let found = env.shell(try_).output().unwrap();
        assert!(found.status.success() && !found.stdout.is_empty(), "{try_}: {}", text(&found.stderr));
    }
}

#[test]
fn ref_words_search_the_installed_api_with_file_and_line() {
    let Some(env) = Env::new() else { return };
    let out = env.bnx(&["ref", "get_functions_at"], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = text(&out.stdout);
    let line =
        out.lines().find(|l| l.starts_with("binaryninja.BinaryView.get_functions_at(addr: int) -> ")).unwrap_or_else(|| panic!("{out}"));
    let (file, at) = line.split(" | ").nth(1).unwrap().rsplit_once(':').unwrap();
    let def = fs::read_to_string(file).unwrap().lines().nth(at.parse::<usize>().unwrap() - 1).unwrap().to_owned();
    assert!(def.trim_start().starts_with("def get_functions_at("), "{line} points at {def:?}");
    assert!(out.contains("\n    :param int addr: "), "an exact name shows its docstring: {out}");
    let rename = text(&env.bnx(&["ref", "rename", "function"], &[]).stdout);
    assert!(rename.starts_with("# ref pitfalls: rename a function\n"), "{rename}");
}

#[test]
fn mcp_serves_exec_and_ref_over_stdio() {
    let Some(env) = Env::new() else { return };
    let mut child = env.command(&["mcp"], &[]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let call = |id: u32, name: &str, args: Value| {
        serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}}).to_string()
    };
    let lines = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_owned(),
        call(3, "exec", serde_json::json!({"target": "w/true", "code": "print('hi')\nlen(bv.functions) > 0"})),
        call(4, "exec", serde_json::json!({"target": "w/true", "code": "nme"})),
        call(5, "ref", serde_json::json!({"query": "get_code_refs"})),
        call(6, "list", serde_json::json!({})),
        call(7, "close", serde_json::json!({"target": "w/true"})),
    ];
    child.stdin.take().unwrap().write_all((lines.join("\n") + "\n").as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<Value> = text(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 7, "{replies:?}");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
    let names: Vec<&str> = replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["exec", "ref", "save", "close", "list"]);
    let body = |i: usize| replies[i]["result"]["content"][0]["text"].as_str().unwrap().to_owned();
    assert_eq!(body(2), "hi\ntrue\n", "{:?}", replies[2]);
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(body(3).contains("Error: script_error: ") && body(3).contains("\nhint: "), "{}", body(3));
    assert!(body(4).contains("binaryninja.BinaryView.get_code_refs(addr: int"), "{}", body(4));
    assert!(body(5).contains("true"), "{}", body(5));
    assert_eq!(replies[6]["result"]["isError"], false, "{}", body(6));
}

#[test]
fn skills_prints_the_bundled_skill_whole_even_to_an_agent() {
    let Some(env) = Env::new() else { return };
    let skill = include_str!("../skills/bnx/SKILL.md");
    for args in [&["skills"][..], &["skills", "show"], &["skills", "show", "SKILL.md"]] {
        // A window far smaller than the skill: it must not apply.
        let out = env.bnx(args, &[("CLAUDECODE", "1"), ("BNX_MAX_BYTES", "200")]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        assert_eq!(text(&out.stdout), skill, "{args:?}");
    }
    assert_eq!(text(&env.bnx(&["skills", "list"], &[]).stdout), format!("# path | bytes\nSKILL.md | {}\n", skill.len()));
    let version = format!("metadata:\n  version: \"{}\"\n", env!("CARGO_PKG_VERSION"));
    assert!(skill.contains(&version), "the skill's version is bnx's, for an agent to compare");
    let help = text(&env.bnx(&["--help"], &[]).stdout);
    assert!(help.contains("`bnx skills` prints the agent skill (SKILL.md); install it where your harness loads skills."), "{help}");
}

/// Every function as a table: enough rows to cut.
const FUNCTIONS: &str = "[{\"addr\": hex(f.start), \"name\": f.name, \"size\": f.total_bytes} for f in bv.functions]";

const BNX_CORPUS: &[(&str, &str)] = &[
    ("src/ref/exec.md", include_str!("../src/ref/exec.md")),
    ("src/ref/session.md", include_str!("../src/ref/session.md")),
    ("src/ref/pitfalls.md", include_str!("../src/ref/pitfalls.md")),
];
