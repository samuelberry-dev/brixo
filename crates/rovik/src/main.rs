//! Command-line runner: `rovik script.rvk`

use std::process::ExitCode;

use rovik::{Interpreter, Trigger};

fn main() -> ExitCode {
    // Scripts get their own thread with a big stack; see SCRIPT_STACK_SIZE.
    rovik::on_script_thread(run_cli)
}

fn run_cli() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: rovik <script.rvk>");
        return ExitCode::FAILURE;
    };

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("couldn't read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut interp = Interpreter::new();
    interp.echo = true;

    if let Err(e) = interp.run_source(&source) {
        eprintln!("error in {path}, {e}");
        return ExitCode::FAILURE;
    }

    if !interp.handlers.is_empty() {
        let events = interp
            .handlers
            .iter()
            .filter(|h| matches!(h.trigger, Trigger::Event(_)))
            .count();
        let timers = interp.handlers.len() - events;
        eprintln!(
            "\n(set up {events} 'on' and {timers} 'every' block(s). These run once the script is inside Brixo.)"
        );
    }
    ExitCode::SUCCESS
}
