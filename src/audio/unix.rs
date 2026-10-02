//! Command-line fallback without build-time speech/audio libraries.
use super::Backend;
use anyhow::{bail, Context, Result};
use std::{
    io::Write,
    process::{Child, Command, Stdio},
};

pub(super) struct SystemSpeech {
    child: Option<Child>,
    voice: Option<String>,
}
impl SystemSpeech {
    pub(super) fn with_voice(voice: Option<String>) -> Result<Self> {
        Ok(Self { child: None, voice })
    }
}
impl Backend for SystemSpeech {
    fn speak(&mut self, text: &str, speed: u16) -> Result<()> {
        self.stop()?;
        let voice = self.voice.clone().or_else(|| {
            std::env::var("TLEGADO_TTS_VOICE")
                .ok()
                .filter(|v| !v.is_empty())
        });
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = Command::new("/usr/bin/say");
            command.args(["-r", &(175 * u32::from(speed) / 100).to_string(), "-f", "-"]);
            if let Some(voice) = &voice {
                command.args(["-v", voice]);
            }
            command
        };
        #[cfg(not(target_os = "macos"))]
        let mut command = {
            let mut command = Command::new("espeak-ng");
            command.args([
                "--stdin",
                "-s",
                &(175 * u32::from(speed) / 100).to_string(),
                "-v",
                voice.as_deref().unwrap_or("cmn"),
            ]);
            command
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("启动系统语音失败：macOS 需要 say，Linux 请安装 espeak-ng")?;
        // Chunks are at most 1,920 UTF-8 bytes, below the supported platforms' pipe capacity.
        let write = child
            .stdin
            .take()
            .context("无法打开语音输入")
            .and_then(|mut stdin| Ok(stdin.write_all(text.as_bytes())?));
        if let Err(error) = write {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        self.child = Some(child);
        Ok(())
    }
    fn pause(&mut self, paused: bool) -> Result<()> {
        if let Some(child) = &mut self.child {
            if child.try_wait()?.is_none() {
                let signal = if paused { libc::SIGSTOP } else { libc::SIGCONT };
                // Only signal our unreaped direct child; its pid cannot be reused here.
                if unsafe { libc::kill(child.id() as libc::pid_t, signal) } != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
        Ok(())
    }
    // CLI backends apply rate changes at the next synthesis chunk.
    fn rate(&mut self, _speed: u16) -> Result<()> {
        Ok(())
    }
    fn finished(&mut self) -> Result<bool> {
        let Some(child) = &mut self.child else {
            return Ok(true);
        };
        match child.try_wait()? {
            Some(status) if status.success() => {
                self.child = None;
                Ok(true)
            }
            Some(_) => {
                self.child = None;
                bail!("系统语音命令执行失败，请检查声音名称和音频设备");
            }
            None => Ok(false),
        }
    }
    fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
        }
        Ok(())
    }
}
impl Drop for SystemSpeech {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
