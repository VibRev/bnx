//! The Python console of Binary Ninja's `python3` scripting provider, bound
//! to the worker's view.
//!
//! The Rust API wraps none of the scripting C API, so this is the raw calls.
//! The console compiles its input like a REPL: a multi-line script fails, and
//! a failed compound statement leaves it waiting for continuation lines. So
//! the console only ever gets one-line calls into the prelude, and requests
//! and answers go through files in the worker's private directory; the
//! answer file appearing (by rename) is the completion signal.

use binaryninja::binary_view::BinaryView;
use binaryninjacore_sys::{
    BNCancelScriptInput, BNCreateScriptingProviderInstance, BNExecuteScriptInput, BNGetScriptingInstanceInputReadyState,
    BNGetScriptingProviderByAPIName, BNRegisterScriptingInstanceOutputListener, BNScriptingInstance, BNScriptingOutputListener,
    BNScriptingProviderExecuteResult, BNScriptingProviderInputReadyState, BNSetScriptingInstanceCurrentBinaryView,
};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PRELUDE: &str = include_str!("prelude.py");

/// Console output that did not go through the prelude's capture: errors in
/// the one-line submissions themselves.
const STRAY_LIMIT: usize = 64 * 1024;

/// How long a submission waits for the console to take input: the first
/// one waits for the interpreter to boot, which is slow on a loaded machine.
const READY_WAIT: Duration = Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    #[error("Binary Ninja registered no `python3` scripting provider (is the bundled Python plugin present?)")]
    NoProvider,
    #[error("the python3 scripting provider refused to create an instance")]
    NoInstance,
    #[error("the Python console did not take the submission: {0}")]
    Rejected(&'static str),
    #[error("the prelude did not install: {0}")]
    Prelude(String),
    #[error("cannot exchange files with the console in {}: {source}", dir.display())]
    Io {
        dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Text the console wrote outside the prelude's capture.
#[derive(Default)]
struct Stray(Mutex<String>);

impl Stray {
    fn push(&self, text: &str) {
        let mut buf = self.0.lock().expect("console output mutex poisoned");
        if buf.len() < STRAY_LIMIT {
            buf.push_str(text);
        }
    }

    fn take(&self) -> String {
        std::mem::take(&mut *self.0.lock().expect("console output mutex poisoned"))
    }
}

unsafe extern "C" fn on_text(ctxt: *mut c_void, text: *const c_char) {
    if ctxt.is_null() || text.is_null() {
        return;
    }
    // SAFETY: `ctxt` is the `Stray` this module registered, alive as long as
    // the console; `text` is a NUL-terminated string valid for the call.
    let (stray, text) = unsafe { (&*ctxt.cast::<Stray>(), CStr::from_ptr(text)) };
    stray.push(&text.to_string_lossy());
}

unsafe extern "C" fn on_ready(_ctxt: *mut c_void, _state: binaryninjacore_sys::BNScriptingProviderInputReadyState) {}

/// One console for the worker's lifetime: the interpreter is per process.
pub struct Console {
    instance: *mut BNScriptingInstance,
    /// Registered with the core, which keeps the pointer: boxed so it never
    /// moves, and never dropped before the process exits.
    _listener: Box<BNScriptingOutputListener>,
    stray: Box<Stray>,
    dir: PathBuf,
    next: u64,
}

impl Console {
    /// Start the interpreter, bind `bv` to `view` and install the prelude.
    ///
    /// # Errors
    ///
    /// No provider, the interpreter did not come up, or the prelude failed.
    pub fn new(view: &BinaryView, dir: &Path) -> Result<Console, ConsoleError> {
        // SAFETY: a literal C string; the provider is owned by the core.
        let provider = unsafe { BNGetScriptingProviderByAPIName(c"python3".as_ptr()) };
        if provider.is_null() {
            return Err(ConsoleError::NoProvider);
        }
        // SAFETY: `provider` is non-null and lives as long as the core.
        let instance = unsafe { BNCreateScriptingProviderInstance(provider) };
        if instance.is_null() {
            return Err(ConsoleError::NoInstance);
        }
        let stray = Box::new(Stray::default());
        let mut listener = Box::new(BNScriptingOutputListener {
            context: std::ptr::from_ref::<Stray>(&stray).cast_mut().cast(),
            output: Some(on_text),
            warning: Some(on_text),
            error: Some(on_text),
            inputReadyStateChanged: Some(on_ready),
        });
        // SAFETY: `instance` is live; `listener` and the `Stray` it points at
        // are boxed and owned by the returned console for the process's life;
        // `view.handle` is a live view the worker keeps open.
        unsafe {
            BNRegisterScriptingInstanceOutputListener(instance, listener.as_mut());
            BNSetScriptingInstanceCurrentBinaryView(instance, view.handle);
        }
        let mut console = Console { instance, _listener: listener, stray, dir: dir.to_path_buf(), next: 0 };
        console.install()?;
        Ok(console)
    }

    fn install(&mut self) -> Result<(), ConsoleError> {
        let prelude = self.dir.join("prelude.py");
        std::fs::write(&prelude, PRELUDE).map_err(|source| ConsoleError::Io { dir: self.dir.clone(), source })?;
        let ready = self.dir.join("ready");
        let line = format!(
            "exec(compile(open({p}).read(), '<bnx-prelude>', 'exec'), globals()); __bnx_run = __bnx_install(bv); __bnx_ready({r})",
            p = py_str(&prelude),
            r = py_str(&ready),
        );
        self.submit(&line)?;
        // The first submission waits for the interpreter to boot and import
        // the binaryninja module.
        if wait_for(&ready, Duration::from_secs(120), || false) {
            Ok(())
        } else {
            let said = self.stray.take();
            Err(ConsoleError::Prelude(if said.trim().is_empty() { "no answer within 120 s".to_owned() } else { said.trim().to_owned() }))
        }
    }

    /// Run one request file and return the answer the prelude wrote to
    /// `answer`. `cancelled` is polled while waiting: once it says so the
    /// script gets a `KeyboardInterrupt`, which the prelude answers like any
    /// exception. A script stuck inside a core call never answers; the daemon
    /// kills the worker then.
    ///
    /// # Errors
    ///
    /// The console refused the submission, or the answer cannot be read.
    pub fn run(&mut self, request: &Path, answer: &Path, mut cancelled: impl FnMut() -> bool) -> Result<String, ConsoleError> {
        self.stray.take();
        self.submit(&format!("__bnx_run({})", py_str(request)))?;
        let mut interrupted = false;
        let done = wait_for(answer, Duration::MAX, || {
            if !interrupted && cancelled() {
                interrupted = true;
                // SAFETY: `instance` is live; cancelling is valid at any time.
                unsafe { BNCancelScriptInput(self.instance) };
            }
            false
        });
        debug_assert!(done, "an unbounded wait only ends with the answer");
        let text = std::fs::read_to_string(answer).map_err(|source| ConsoleError::Io { dir: self.dir.clone(), source })?;
        let _ = std::fs::remove_file(answer);
        Ok(text)
    }

    /// A fresh pair of request and answer paths.
    pub fn paths(&mut self) -> (PathBuf, PathBuf) {
        self.next += 1;
        (self.dir.join(format!("req-{}.json", self.next)), self.dir.join(format!("ans-{}.json", self.next)))
    }

    /// Submit one line once the console takes input. A console that is not
    /// ready (booting, or still returning from the previous submission
    /// after its answer appeared) refuses input as invalid.
    fn submit(&self, line: &str) -> Result<(), ConsoleError> {
        let line = CString::new(line).map_err(|_| ConsoleError::Rejected("it contains a NUL byte"))?;
        let start = Instant::now();
        // SAFETY: `instance` is live for the console's lifetime.
        while unsafe { BNGetScriptingInstanceInputReadyState(self.instance) } != BNScriptingProviderInputReadyState::ReadyForScriptExecution
        {
            if start.elapsed() >= READY_WAIT {
                return Err(ConsoleError::Rejected("the console was not ready for input within 120 s"));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        // SAFETY: `instance` is live and `line` is NUL-terminated.
        match unsafe { BNExecuteScriptInput(self.instance, line.as_ptr()) } {
            BNScriptingProviderExecuteResult::SuccessfulScriptExecution => Ok(()),
            BNScriptingProviderExecuteResult::ScriptExecutionCancelled => Err(ConsoleError::Rejected("cancelled before it ran")),
            BNScriptingProviderExecuteResult::IncompleteScriptInput => Err(ConsoleError::Rejected("buffered as an incomplete statement")),
            BNScriptingProviderExecuteResult::InvalidScriptInput => Err(ConsoleError::Rejected("invalid input")),
        }
    }
}

/// `path` as a Python string literal: JSON's escaping is valid Python.
fn py_str(path: &Path) -> String {
    serde_json::Value::String(path.to_string_lossy().into_owned()).to_string()
}

/// Poll for `path` until `budget` runs out; `tick` runs between polls and
/// ends the wait early by returning true.
fn wait_for(path: &Path, budget: Duration, mut tick: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if path.exists() {
            return true;
        }
        if tick() || start.elapsed() >= budget {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_become_python_literals() {
        assert_eq!(py_str(Path::new("/a/b \"c\".json")), r#""/a/b \"c\".json""#);
    }
}
