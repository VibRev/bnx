//! bnx: Binary Ninja for agents (README.md). The commands, sessions and
//! daemon are rex's; Binary Ninja itself is [`backend::BinaryNinja`].

mod backend;

use axkit::skills::SkillFile;
use rex::pyapi::Naming;
use rex::reference::{Docs, Place, Role};
use rex::save::Databases;
use rex::tool::Tool;
use std::process::ExitCode;

static BNX: Tool = Tool {
    name: "bnx",
    version: env!("CARGO_PKG_VERSION"),
    engine: "Binary Ninja",
    databases: Databases { extensions: &["bndb"] },
    about: "Binary Ninja for agents: sessions stay open in a daemon, questions are Python scripts",
    examples: "\
Examples:
  bnx exec ./fw.bin 'len(bv.functions)'                # opens the target on demand
  bnx exec ./fw.bin 'str(bv.get_functions_by_name(\"main\")[0].hlil)'
  bnx exec ./fw.bin 'for f in bv.functions:
      if f.total_bytes > 1000: print(hex(f.start), f.name)'   # several lines in one argument
  bnx exec ./fw.bin -f script.py                       # a file; - reads the code from stdin
  bnx ref code refs                                    # search the binaryninja API, no session needed
  bnx save ./fw.bin && bnx close ./fw.bin              # nothing is written without save

The view stays open in a background daemon between commands. Nothing is
written next to the target until `bnx save`.

Exit status: 0 ok, 1 error, 3 session lost or timeout.",
    exec_help: "\
The code is the argument after the target: one argument, as many lines as it
needs. -c CODE says the same (several -c run as one script), -f FILE runs a
file, and - (or -f -) reads the code from stdin.

In the code: bv is the BinaryView and everything from `binaryninja` is
imported. Each exec starts with fresh globals. The answer is the value of the
last expression, after anything printed; objects that are not JSON print as
their str(). --json prints {\"stdout\", \"result\", \"analysis\"}.

Unsure of an API? `bnx ref <words>` before guessing.

Examples:
  bnx exec fw.bin 'len(bv.functions)'
  bnx exec fw.bin 'for f in bv.functions:
      if f.total_bytes > 1000: print(hex(f.start), f.name)'
  bnx exec fw.bin -f script.py",
    scripting: "Python with bv (the BinaryView) and everything from binaryninja imported",
    corpus: &[include_str!("ref/exec.md"), include_str!("ref/session.md"), include_str!("ref/pitfalls.md")],
    docs: Docs {
        library: "libbinaryninjacore",
        env: "BINARYNINJADIR",
        naming: Naming::Package("binaryninja"),
        places: &[
            Place {
                role: Role::Api,
                paths: &["python/binaryninja", "../Resources/python/binaryninja"],
                holds: "API signatures and docstrings: the binaryninja package",
                tries: &["grep -n -A4 'def get_functions_by_name' {}/binaryview.py", "grep -ln 'def update_analysis_and_wait' {}/*.py"],
            },
            Place {
                role: Role::Examples,
                paths: &["examples/python", "../Resources/examples/python"],
                holds: "official examples",
                tries: &["grep -rln get_code_refs {}", "grep -rln '[.]hlil' {}"],
            },
        ],
    },
    skill: &[SkillFile { path: "SKILL.md", text: include_str!("../skills/bnx/SKILL.md") }],
};

fn main() -> ExitCode {
    rex::tool::main::<backend::BinaryNinja>(&BNX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rex::reference::Corpus;

    #[test]
    fn every_entry_starts_with_a_description() {
        let corpus = Corpus::parse(BNX.corpus);
        assert_eq!(corpus.topics().len(), BNX.corpus.len(), "a text without a `# topic: about` line");
        let entries: Vec<_> = corpus.topics().iter().flat_map(|t| &t.entries).collect();
        assert!(entries.len() > 5, "{}", entries.len());
        for e in entries {
            assert!(e.body.lines().next().is_some_and(|l| !l.is_empty() && !l.starts_with(' ')), "{e:?}");
        }
    }

    #[test]
    fn the_command_line_is_consistent() {
        rex::cli::Cli::command_for(&BNX).debug_assert();
    }
}
