// OpenCode plugin installation.
//
// OpenCode has no command hooks like Claude Code: it runs JavaScript plugins
// from `~/.config/opencode/plugins/`. So instead of editing a settings file,
// Coucou drops a plugin file there — `coucou.js`, one file, ours only.
//
// The plugin is the bridge the whole relay architecture needs. It is a pure
// forwarder: OpenCode calls its hooks, the plugin serialises what happened to
// JSON and runs `coucou-hook --agent opencode <EventName>` — the very same
// relay Claude Code uses, tagged so the island routes the events to the
// `agent_opencode` pill. The relay's pipe, timeouts and silence rules all
// apply unchanged; a session is never blocked by Coucou.
//
// Same contract as the Claude Code installer: preview before write, dated
// backup, fingerprint so only the bytes the user looked at are written, and
// uninstall removes only our file.

use std::path::PathBuf;

use serde::Serialize;

use crate::platform;

/// How the installed plugin identifies itself: the export name inside the
/// plugin file and the file name itself.
pub const PLUGIN_FILE: &str = "coucou.js";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeStatus {
    pub installed: bool,
    pub config_dir: String,
    pub plugin_path: String,
    /// The relay exists in bin/ — without it the plugin would spawn nothing.
    pub hook_ready: bool,
    /// OpenCode itself was detected (its config directory exists).
    pub app_detected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodePreview {
    /// What the user sees before anything is written: the whole plugin file,
    /// or "removed" on uninstall.
    pub diff: String,
    pub backup: String,
    pub plugin_path: String,
    /// Identifies the bytes this preview was computed from; handed back to
    /// `write` so we only ever apply what the user actually looked at.
    pub fingerprint: String,
}

/// `~/.config/opencode/` on both platforms OpenCode supports.
pub fn config_dir() -> PathBuf {
    // OpenCode reads XDG_CONFIG_HOME/OPENCODE_CONFIG when set, like its own
    // docs describe; fall back to ~/.config/opencode.
    if let Some(dir) = std::env::var_os("OPENCODE_CONFIG") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return dir;
        }
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| platform::home_dir().join(".config"));
    base.join("opencode")
}

pub fn plugin_path() -> PathBuf {
    config_dir().join("plugins").join(PLUGIN_FILE)
}

/// The hook exe path exactly as the plugin will spawn it: the relay copied to
/// bin/ on launch, same as the Claude Code installer points at.
fn hook_spawn_path() -> PathBuf {
    crate::settings::hook_exe_path()
}

/// The plugin body. One responsibility: forward OpenCode's hook calls to the
/// relay. Everything else — the pipe, the deadlines, the decision budget —
/// already lives in coucou-hook.
///
/// Kept as a raw string so the file we ship is byte-for-byte the file we
/// showed in the preview.
fn plugin_source() -> String {
    let exe = hook_spawn_path().to_string_lossy().replace('\\', "/");
    // The plugin body is plain JavaScript with braces of its own, so it is a
    // const template with one placeholder rather than a format! string.
    PLUGIN_TEMPLATE.replace("@COUCOU_RELAY@", &exe)
}

/// The plugin body. One responsibility: forward OpenCode's hook calls to the
/// relay. Everything else — the pipe, the deadlines, the decision budget —
/// already lives in coucou-hook.
///
/// Kept verbatim so the file we ship is byte-for-byte the file we showed in
/// the preview.
const PLUGIN_TEMPLATE: &str = r#"// Coucou for OpenCode — installed and updated by the Coucou app.
// This file is Coucou's; removing it removes the integration. Everything else
// in this folder is left alone.
//
// It forwards OpenCode's hooks to the Coucou relay, which never blocks the
// session: if Coucou is closed the relay exits immediately and OpenCode
// carries on as if this plugin did not exist.

const RELAY = "@COUCOU_RELAY@"
const AGENT = "opencode"

const { spawn } = await import("node:child_process")

/** Maps an OpenCode event to the island's event names. Null → not forwarded.
 *  The island's states (working/thinking/finished/error…) are driven by the
 *  Claude Code event set, so OpenCode's events are translated onto it. */
function hookName(event) {
  switch (event) {
    case "tool.execute.before": return "PreToolUse"
    case "tool.execute.after": return "PostToolUse"
    case "session.created": return "SessionStart"
    case "session.idle": return "Stop"
    case "session.error": return "StopFailure"
    case "session.deleted": return "SessionEnd"
    default: return null
  }
}

/** Sends one payload to the relay tagged with the mapped island event.
 *  The relay stamps hook_event_name from argv; the plugin sends the fields
 *  the island reads (tool_name, tool_input, cwd…). */
function forward(event, payload) {
  const mapped = hookName(event)
  if (!mapped) return
  let body
  try {
    body = JSON.stringify(payload ?? {})
  } catch {
    body = "{}"
  }
  const child = spawn(RELAY, ["--agent", AGENT, mapped], {
    stdio: ["pipe", "ignore", "ignore"],
    windowsHide: true,
  })
  child.on("error", () => {}) // Coucou closed: spawn fails, session continues
  child.stdin.on("error", () => {})
  child.stdin.write(body)
  child.stdin.end()
}

export const CoucouPlugin = async (ctx) => {
  // OpenCode does not emit a "session start" hook: this is the moment the
  // plugin loads, which is per session and carries the directory.
  forward("session.created", { cwd: ctx.directory })

  return {
    "tool.execute.before": async (input) => {
      // The island reads tool_name / tool_input; OpenCode gives tool / args.
      forward("tool.execute.before", {
        tool_name: input.tool,
        tool_input: input.args,
        session_id: input.sessionID,
        cwd: ctx.directory,
      })
    },
    "tool.execute.after": async (input) => {
      forward("tool.execute.after", {
        tool_name: input.tool,
        session_id: input.sessionID,
        cwd: ctx.directory,
      })
    },
    event: async ({ event }) => {
      const p = event.properties ?? {}
      forward(event.type, {
        // message is what Notification/Stop read; sessions carry an id.
        message: typeof p.error === "string" ? p.error : undefined,
        session_id: p.sessionID ?? p.id,
        cwd: ctx.directory,
      })
    },
  }
}
"#;

/// Reads the plugin file, if it exists.
fn read_plugin() -> Result<Option<String>, String> {
    match std::fs::read_to_string(plugin_path()) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("Can't read {}: {err}", plugin_path().display())),
    }
}

fn read_plugin_lossy() -> Option<String> {
    read_plugin().ok().flatten()
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn status() -> OpenCodeStatus {
    let installed = read_plugin_lossy()
        .map(|text| is_ours(&text))
        .unwrap_or(false);
    OpenCodeStatus {
        installed,
        config_dir: config_dir().to_string_lossy().to_string(),
        plugin_path: plugin_path().to_string_lossy().to_string(),
        hook_ready: hook_spawn_path().exists(),
        app_detected: config_dir().exists(),
    }
}

/// A Coucou plugin is one that exports `CoucouPlugin` — the marker this
/// uninstall looks for.
fn is_ours(text: &str) -> bool {
    text.contains("CoucouPlugin")
}

pub fn preview(install: bool) -> Result<OpenCodePreview, String> {
    let current = read_plugin()?;
    let next = if install {
        Some(plugin_source())
    } else {
        None // uninstall removes the file
    };
    let diff = match (&current, &next) {
        (Some(a), Some(b)) if a == b => {
            "No changes — the plugin is already installed and up to date.".to_string()
        }
        (Some(a), Some(b)) => unified_diff(a, b),
        (None, Some(b)) => format!("+ install the plugin (~{} bytes):\n\n{}", b.len(), b),
        (Some(_), None) => {
            "− remove the plugin file. Nothing else in this folder is touched.".to_string()
        }
        (None, None) => "Nothing to do.".to_string(),
    };
    Ok(OpenCodePreview {
        diff,
        backup: backup_path().to_string_lossy().to_string(),
        plugin_path: plugin_path().to_string_lossy().to_string(),
        fingerprint: current_fingerprint(),
    })
}

/// Writes (or removes) the plugin after taking a dated backup of the previous
/// file, refusing if it changed since the preview.
pub fn write(install: bool, fingerprint: &str) -> Result<String, String> {
    if current_fingerprint() != fingerprint {
        return Err(format!(
            "{} changed since you looked at the preview. Please review the new version and try again.",
            plugin_path().display()
        ));
    }

    if install {
        let path = plugin_path();
        let Some(dir) = path.parent() else {
            return Err("the plugin directory has no parent".into());
        };
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }

    let backup = backup_path();
    if let Some(current) = read_plugin()? {
        // Only back up when there is something to lose.
        if backup.exists() {
            let _ = std::fs::remove_file(&backup);
        }
        std::fs::write(&backup, &current)
            .map_err(|e| format!("could not write {}: {e}", backup.display()))?;
    }

    if install {
        std::fs::write(plugin_path(), plugin_source())
            .map_err(|e| format!("could not write {}: {e}", plugin_path().display()))?;
    } else {
        match std::fs::remove_file(plugin_path()) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "could not remove {}: {err}",
                    plugin_path().display()
                ))
            }
        }
    }
    Ok(backup.to_string_lossy().to_string())
}

/// The Claude Code installer's stamp, down to the same second resolution.
fn stamp() -> String {
    let t = platform::local_time();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

fn backup_path() -> PathBuf {
    plugin_path().with_file_name(format!("{PLUGIN_FILE}.bak-{}", stamp()))
}

/// Same FNV-1a as the Claude Code installer: the question is only "is this
/// still the file I showed the user?".
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn current_fingerprint() -> String {
    match std::fs::read(plugin_path()) {
        Ok(bytes) => fingerprint(&bytes),
        Err(_) => fingerprint(b""),
    }
}

/// Line-based unified diff, same simple honest approach as the Claude Code
/// installer (settings files are short; so is a plugin file).
fn unified_diff(a: &str, b: &str) -> String {
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();

    // Common prefix / suffix trim, then one change block. Good enough for a
    // ~70-line file a human reads to see what will run on their machine.
    let mut start = 0;
    while start < a_lines.len() && start < b_lines.len() && a_lines[start] == b_lines[start] {
        start += 1;
    }
    let mut end_a = a_lines.len();
    let mut end_b = b_lines.len();
    while end_a > start && end_b > start && a_lines[end_a - 1] == b_lines[end_b - 1] {
        end_a -= 1;
        end_b -= 1;
    }

    let mut out = String::new();
    for line in &a_lines[start..end_a] {
        out.push('-');
        out.push_str(line);
        out.push('\n');
    }
    for line in &b_lines[start..end_b] {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_forwards_to_the_relay_tagged_as_opencode() {
        let src = plugin_source();
        assert!(
            src.contains("--agent"),
            "must tag events for the pill routing"
        );
        assert!(src.contains("\"opencode\""), "the tag is opencode");
        assert!(src.contains("CoucouPlugin"), "must export the marker name");
        assert!(src.contains("windowsHide: true"));
        // The relay path must be embedded, forward slashes or not.
        assert!(src.contains("coucou-hook"));
    }

    #[test]
    fn is_ours_recognises_only_our_export() {
        assert!(is_ours("export const CoucouPlugin = 1"));
        assert!(!is_ours("export const SomethingElse = 1"));
        assert!(!is_ours(""));
    }

    #[test]
    fn unified_diff_shows_changed_lines() {
        let d = unified_diff("a\nb\nc\n", "a\nB\nc\n");
        assert!(d.contains("-b"));
        assert!(d.contains("+B"));
        assert!(!d.contains("-a"));
    }

    #[test]
    fn fingerprint_distinguishes_bytes() {
        assert_ne!(fingerprint(b"one"), fingerprint(b"two"));
        assert_eq!(fingerprint(b""), fingerprint(b""));
    }

    /// The plugin must survive a real JS parser: a syntax error would only
    /// surface inside OpenCode, at session start, with no Coucou log to read.
    #[test]
    fn the_plugin_is_parseable_javascript() {
        let dir = std::env::temp_dir().join(format!("coucou-plugin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("coucou-plugin.mjs");
        std::fs::write(&file, plugin_source()).unwrap();
        let out = std::process::Command::new("node")
            .arg("--check")
            .arg(&file)
            .output()
            .expect("node must be available");
        std::fs::remove_dir_all(&dir).ok();
        assert!(
            out.status.success(),
            "the generated plugin has a syntax error:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Every event the plugin maps must be an event the island handles — a
    /// mismatched name means the pill never reacts and nothing errors.
    #[test]
    fn the_plugin_only_maps_events_the_island_knows() {
        let src = plugin_source();
        let island = [
            "SessionStart",
            "PreToolUse",
            "PostToolUse",
            "Notification",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ];
        for line in src.lines() {
            let Some(idx) = line.find("return \"") else {
                continue;
            };
            let rest = &line[idx + 8..];
            let Some(end) = rest.find('"') else { continue };
            let mapped = &rest[..end];
            // Names in the switch's default case are OpenCode's, not ours.
            if line.contains("case ") && line.contains(": return") {
                assert!(
                    island.contains(&mapped),
                    "the plugin maps to {mapped}, which the island does not handle"
                );
            }
        }
    }
}
