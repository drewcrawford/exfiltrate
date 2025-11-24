#![cfg(not(target_arch = "wasm32"))]
use exfiltrate_internal::command::{Command, Response};

/// The `terminate` command.
///
/// Exits the application with status code 70 (EX_SOFTWARE).
/// Only available on native targets.
pub struct Terminate;

impl Command for Terminate {
    fn name(&self) -> &'static str {
        "terminate"
    }

    fn short_description(&self) -> &'static str {
        "Terminates the program being debugged.  Use this to kill programs that stay resident."
    }

    fn full_description(&self) -> &'static str {
        "Terminates the program being debugged.  Use this to kill programs that stay resident.

A common debugging workflow is:
1.  build/run
2.  examine the program interactively
3.  quit the program

However step 3 may be difficult in a sandbox, or require PID tracking, etc.

This command will remotely exit the program we are debugging, with exit code 70 (EX_SOFTWARE)."
    }

    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        std::thread::Builder::new()
            .name("terminate".to_owned())
            .spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(50));
                std::process::exit(70 /* EX_SOFTWARE */);
            })
            .unwrap();
        Ok("Termination successful.".into())
    }
}
