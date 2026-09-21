//! Rovik, the scripting language for Brixo.
//!
//! Source text goes through three stages:
//! lexer (text -> tokens), parser (tokens -> syntax tree),
//! interpreter (runs the tree).

pub mod ast;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod value;

pub use error::RovikError;
pub use interpreter::{Handler, Interpreter, PrintHook, Trigger, WaitHook};
pub use value::{Host, ObjectRef, Value};

/// Stack size for the thread that runs scripts.
///
/// This interpreter walks the syntax tree recursively, so every nested
/// Rovik function call uses real Rust stack (about 10-20 KB each in a debug
/// build). Windows only gives the main thread 1 MB, which runs out after
/// roughly 50 nested calls. 64 MB is reserved address space, not memory
/// actually used. The planned bytecode VM removes this limitation.
pub const SCRIPT_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Runs `f` on a thread with enough stack for deeply nested scripts.
pub fn on_script_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .name("rovik".to_string())
        .stack_size(SCRIPT_STACK_SIZE)
        .spawn(f)
        .expect("couldn't start the script thread")
        .join()
        .expect("the script thread crashed")
}

/// Runs a script and returns everything it printed.
pub fn run(source: &str) -> Result<Vec<String>, RovikError> {
    let source = source.to_string();
    on_script_thread(move || {
        let mut interp = Interpreter::new();
        interp.run_source(&source)?;
        Ok(interp.output)
    })
}
