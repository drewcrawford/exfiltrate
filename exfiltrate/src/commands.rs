use crate::command::Command;
use wasm_safe_mutex::rwlock::RwLock;

mod help;
mod list;
mod terminate;

/// The global registry of available commands.
///
/// This list is populated by `register_commands` and `exfiltrate::add_command`.
pub(crate) static COMMANDS: RwLock<Vec<Box<dyn Command>>> = RwLock::new(vec![]);

/// Registers the built-in commands (help, list, terminate).
///
/// This is called automatically by `exfiltrate::begin()`.
pub(crate) fn register_commands() {
    let mut lock = COMMANDS.lock_sync_write();
    lock.push(Box::new(help::Help));
    lock.push(Box::new(list::List));
    #[cfg(not(target_arch = "wasm32"))]
    lock.push(Box::new(terminate::Terminate));
}
