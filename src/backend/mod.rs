//! Binary Ninja as a rex [`Backend`]: one `BinaryView` and its Python
//! console, in the worker process (README, "Process model").

mod console;

use binaryninja::binary_view::{AnalysisProgress, BinaryView};
use binaryninja::file_metadata::SaveSettings;
use binaryninja::headless::Session;
use binaryninja::rc::Ref;
use console::Console;
use rex::analysis::Analysis;
use rex::exec::ScriptRun;
use rex::proto::{ExecRequest, Launch, State, WorkerFailure};
use rex::worker::{Backend, Channel};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The Binary Ninja build this bnx is written and tested against (rex §0.1).
pub const ENGINE: (u32, u32, u32) = (6, 0, 10905);

pub struct BinaryNinja {
    /// Dropped last: the core shuts down with it.
    _session: Session,
    view: Ref<BinaryView>,
    target: PathBuf,
    workdir: PathBuf,
    console: Option<Console>,
}

impl Backend for BinaryNinja {
    /// Analysis starts at once and runs in the background; `analyze` waits
    /// for it.
    fn open(launch: &Launch, _channel: &Channel) -> Result<BinaryNinja, WorkerFailure> {
        let session = Session::new().map_err(|e| {
            WorkerFailure::engine(format!("Binary Ninja did not start: {e:?}"))
                .with_hint("bnx needs a headless-capable license in ~/.binaryninja/license.dat or BN_LICENSE_FILE")
        })?;
        let v = binaryninja::version_info();
        if (v.major, v.minor, v.build) != ENGINE {
            let (major, minor, build) = ENGINE;
            return Err(WorkerFailure::engine(format!(
                "loaded Binary Ninja {}, but this bnx is built for {major}.{minor}.{build}",
                binaryninja::version()
            ))
            .with_hint("rebuild bnx with BINARYNINJADIR pointing at the matching install"));
        }
        let view = session
            .load_with_options(&launch.input, false, Some("{}"))
            .ok_or_else(|| WorkerFailure::target(format!("Binary Ninja cannot open {}", launch.target.display())))?;
        view.update_analysis();
        Ok(BinaryNinja { _session: session, view, target: launch.target.clone(), workdir: launch.workdir.clone(), console: None })
    }

    fn state(&self) -> State {
        let (complete, progress, phase) = match self.view.analysis_progress() {
            AnalysisProgress::Idle => (true, Some(1.0), "idle".to_owned()),
            AnalysisProgress::Initial => (false, Some(0.0), "not started".to_owned()),
            AnalysisProgress::Hold => (false, None, "on hold".to_owned()),
            AnalysisProgress::Discovery => (false, None, "discovery".to_owned()),
            AnalysisProgress::Disassembling(n, total) => (false, fraction(n, total), format!("disassembling {n}/{total}")),
            AnalysisProgress::Analyzing(n, total) => (false, fraction(n, total), format!("analyzing {n}/{total}")),
            AnalysisProgress::ExtendedAnalysis => (false, None, "extended analysis".to_owned()),
        };
        State { analysis: Analysis { complete, progress }, phase: (!complete).then_some(phase) }
    }

    /// Block until analysis is idle, a progress line every 5 s (AX §7). A
    /// cancel stops the wait, not the analysis.
    fn analyze(&mut self, channel: &Channel) {
        let start = Instant::now();
        let mut next_report = Duration::from_secs(5);
        loop {
            let state = self.state();
            if state.analysis.complete || channel.is_cancelled() {
                return;
            }
            if start.elapsed() >= next_report {
                channel.progress(&format!(
                    "# progress: analysis of {} {}, {} functions, {}s elapsed",
                    self.target.display(),
                    state.phase.as_deref().unwrap_or("running"),
                    self.view.functions().len(),
                    start.elapsed().as_secs()
                ));
                next_report += Duration::from_secs(5);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn exec(&mut self, exec: &ExecRequest, channel: &Channel) -> Result<ScriptRun, WorkerFailure> {
        let console = match &mut self.console {
            Some(c) => c,
            None => {
                let started = Console::new(&self.view, &self.workdir)
                    .map_err(|e| WorkerFailure::engine(format!("the Python console did not start: {e}")))?;
                self.console.insert(started)
            }
        };
        let (request, answer) = console.paths();
        let body = serde_json::json!({
            "code": exec.code,
            "filename": exec.filename,
            "answer": answer,
        });
        std::fs::write(&request, body.to_string()).map_err(|e| WorkerFailure::engine(format!("cannot write the script request: {e}")))?;
        let text = console.run(&request, &answer, || channel.is_cancelled()).map_err(|e| WorkerFailure::engine(e.to_string()));
        let _ = std::fs::remove_file(&request);
        serde_json::from_str(&text?).map_err(|e| WorkerFailure::engine(format!("the prelude answered something unreadable: {e}")))
    }

    /// Always a whole new database at `to`: rex moves it into place, and the
    /// next save writes another.
    fn save(&mut self, to: &Path) -> Result<(), WorkerFailure> {
        if self.view.file().create_database(to, &SaveSettings::new()) {
            Ok(())
        } else {
            Err(WorkerFailure::engine(format!("Binary Ninja could not write a database to {}", to.display())))
        }
    }
}

fn fraction(n: usize, total: usize) -> Option<f64> {
    // Counts of functions; far below 2^52, so the conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    (total > 0).then(|| n as f64 / total as f64)
}
