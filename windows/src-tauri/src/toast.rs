// Windows toast notifications for permission requests.
//
// When the island is hidden and Claude Code or OpenCode asks for permission,
// nothing on screen says so. This module puts the question where the user is
// looking: a native Windows toast with Allow / Deny buttons.
//
// Design notes:
// * The toast is shown by the running app itself, in-process — the button
//   callback fires on a WinRT thread and answers through the same
//   pipe::answer path the island's card uses. No second process, no URL
//   scheme, no installed-shortcut AUMID gymnastics.
// * The toast uses POWERSHELL_APP_ID as its AppUserModelID: an AUMID of our
//   own would need a Start-menu shortcut to be toast-eligible, and the
//   installer is not code-signed/distributed yet. The cost is the PowerShell
//   icon on the toast and PowerShell named in Action Center — acceptable
//   until Coucou ships a signed installer with its own AUMID.
// * The crate has no "hide" API: answering from the island does not remove
//   the toast. A late button click is therefore a no-op (the reply path logs
//   "no pending request") — safe, just untidy. Documented, not hidden.
// * Linux compiles this module out entirely; the island card remains the
//   only approval surface there.

use tauri::AppHandle;

#[cfg(windows)]
use crate::log;
#[cfg(windows)]
use crate::pipe;

/// Shows the approval toast for a permission request.
///
/// `request_id` is the pipe's id (the same one the island card answers),
/// `tool` and `command` are what the user is actually authorising, and
/// `agent` is a display name ("Claude Code", "OpenCode") for the title.
#[cfg(windows)]
pub fn approval(app: AppHandle, request_id: String, agent: &str, tool: &str, command: &str) {
    use tauri_winrt_notification::{Duration, Toast};

    let title = format!("{agent} wants permission");
    // One line of context: the whole point of approving from a toast is
    // knowing WHAT you are approving, not just that something wants you.
    let line = if command.is_empty() || command == tool {
        tool
    } else {
        command
    };

    let rid_for_allow = request_id.clone();

    let result = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(&title)
        .text1(&line)
        .duration(Duration::Long)
        .add_button("Allow", "allow")
        .add_button("Deny", "deny")
        .on_activated(move |action| {
            // Runs on a WinRT thread; pipe::answer is thread-safe and takes
            // the app handle by value. Anything else in flight is unaffected.
            let Some(action) = action else { return Ok(()) };
            let decision = if action == "allow" { "allow" } else { "deny" };
            pipe::answer(&app, &rid_for_allow, decision);
            Ok(())
        })
        .show();
    if let Err(err) = result {
        log::line(format!("toast failed: {err}"));
    }
}

#[cfg(not(windows))]
pub fn approval(_app: AppHandle, _request_id: String, _agent: &str, _tool: &str, _command: &str) {}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_keeps_the_module_compiling_off_windows() {
        // The real surface is Windows-only; the stub exists so callers need
        // no cfg of their own.
        assert!(true);
    }
}
