use anyhow::Result;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

const PROMPT_TMP: &str = "/tmp/present_prompt.txt";

pub fn send(content: &str, topic: &str, prompt_idx: usize) -> Result<String> {
    send_with(content, topic, prompt_idx, &RealSender)
}

fn send_with(
    content: &str,
    topic: &str,
    prompt_idx: usize,
    sender: &dyn ClaudeSender,
) -> Result<String> {
    save_to_gen(content, topic, prompt_idx)?;

    match sender.send_via_osascript(content) {
        Ok(()) => Ok("Sent to Claude via iTerm2".to_string()),
        Err(_) => {
            sender.copy_to_clipboard(content)?;
            Ok("Copied to clipboard — paste into Claude (⌘V)".to_string())
        }
    }
}

trait ClaudeSender {
    fn send_via_osascript(&self, content: &str) -> Result<()>;
    fn copy_to_clipboard(&self, content: &str) -> Result<()>;
}

struct RealSender;

impl ClaudeSender for RealSender {
    fn send_via_osascript(&self, content: &str) -> Result<()> {
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

    fn copy_to_clipboard(&self, content: &str) -> Result<()> {
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
}

fn save_to_gen(content: &str, topic: &str, prompt_idx: usize) -> Result<()> {
    let dir = format!("gen/{topic}/{}", prompt_idx + 1);
    fs::create_dir_all(&dir)?;
    fs::write(format!("{dir}/prompt.md"), content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn send_reports_success_without_touching_osascript_or_clipboard() {
        let sender = FakeSender {
            osascript_result: Ok(()),
            clipboard_calls: RefCell::new(0),
        };

        let msg = send_with("hello", "unit-test-topic", 0, &sender).unwrap();

        assert_eq!(msg, "Sent to Claude via iTerm2");
        assert_eq!(*sender.clipboard_calls.borrow(), 0);
    }

    #[test]
    fn send_falls_back_to_clipboard_when_osascript_fails() {
        let sender = FakeSender {
            osascript_result: Err(anyhow::anyhow!("iTerm2 not running")),
            clipboard_calls: RefCell::new(0),
        };

        let msg = send_with("hello", "unit-test-topic", 0, &sender).unwrap();

        assert_eq!(msg, "Copied to clipboard — paste into Claude (⌘V)");
        assert_eq!(*sender.clipboard_calls.borrow(), 1);
    }

    struct FakeSender {
        osascript_result: Result<()>,
        clipboard_calls: RefCell<usize>,
    }

    impl ClaudeSender for FakeSender {
        fn send_via_osascript(&self, _content: &str) -> Result<()> {
            match &self.osascript_result {
                Ok(()) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        }

        fn copy_to_clipboard(&self, _content: &str) -> Result<()> {
            *self.clipboard_calls.borrow_mut() += 1;
            Ok(())
        }
    }
}
