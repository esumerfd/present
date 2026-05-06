use anyhow::Result;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

const PROMPT_TMP: &str = "/tmp/present_prompt.txt";

pub fn send(content: &str, topic: &str, prompt_idx: usize) -> Result<String> {
    save_to_gen(content, topic, prompt_idx)?;

    match send_via_osascript(content) {
        Ok(()) => Ok("Sent to Claude via iTerm2".to_string()),
        Err(_) => {
            copy_to_clipboard(content)?;
            Ok("Copied to clipboard — paste into Claude (⌘V)".to_string())
        }
    }
}

fn send_via_osascript(content: &str) -> Result<()> {
    // Write to temp file so the AppleScript string literal never contains newlines.
    fs::write(PROMPT_TMP, content)?;

    let script = format!(
        r#"tell application "iTerm2"
    activate
    delay 0.15
    tell current window
        tell current session
            write text (do shell script "cat {PROMPT_TMP}")
        end tell
    end tell
end tell"#
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()?;

    anyhow::ensure!(status.success(), "osascript exited non-zero");
    Ok(())
}

fn copy_to_clipboard(content: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(content.as_bytes())?;
    child.wait()?;
    Ok(())
}

fn save_to_gen(content: &str, topic: &str, prompt_idx: usize) -> Result<()> {
    let dir = format!("gen/{topic}/{}", prompt_idx + 1);
    fs::create_dir_all(&dir)?;
    fs::write(format!("{dir}/prompt.md"), content)?;
    Ok(())
}
