use super::Backend;
use anyhow::{bail, Context, Result};
use windows::{
    core::{w, HSTRING, PCWSTR, PWSTR},
    Win32::{Media::Speech::*, System::Com::*},
};

struct Apartment;
impl Apartment {
    fn new() -> Result<Self> {
        // SAPI owns its asynchronous synthesis threads; all of our COM calls stay on this MTA.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub(super) struct SystemSpeech {
    voice: ISpVoice,
    chinese: Option<ISpObjectToken>,
    default: ISpObjectToken,
    explicit: bool,
    paused: bool,
    // COM interfaces must be released before apartment uninitialization.
    _apartment: Apartment,
}

fn owned_string(value: PWSTR) -> Result<String> {
    // SAPI allocates these strings with the COM task allocator.
    let result = unsafe { value.to_string() };
    unsafe {
        CoTaskMemFree(Some(value.0.cast()));
    }
    Ok(result?)
}

impl SystemSpeech {
    #[cfg(test)]
    pub(super) fn new() -> Result<Self> {
        Self::with_voice(None)
    }

    pub(super) fn with_voice(requested: Option<String>) -> Result<Self> {
        let apartment = Apartment::new().context("初始化 Windows COM 失败")?;
        unsafe {
            let voice: ISpVoice =
                CoCreateInstance(&SpVoice, None, CLSCTX_ALL).context("系统 SAPI 不可用")?;
            let default = voice
                .GetVoice()
                .context("没有可用的系统语音，请安装语音包")?;
            let category: ISpObjectTokenCategory =
                CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL)?;
            category.SetId(
                w!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech\\Voices"),
                false,
            )?;
            let tokens = category.EnumTokens(PCWSTR::null(), PCWSTR::null())?;
            let mut count = 0;
            tokens.GetCount(&mut count)?;
            let requested = requested.or_else(|| {
                std::env::var("TLEGADO_TTS_VOICE")
                    .ok()
                    .filter(|v| !v.is_empty())
            });
            let mut chinese = None;
            let mut selected = None;
            let mut names = Vec::new();
            for index in 0..count {
                let token = tokens.Item(index)?;
                let name = owned_string(token.GetStringValue(PCWSTR::null())?)?;
                if requested
                    .as_ref()
                    .is_some_and(|requested| requested.eq_ignore_ascii_case(&name))
                {
                    selected = Some(token.clone());
                }
                let attributes = token.OpenKey(w!("Attributes"))?;
                let language = owned_string(attributes.GetStringValue(w!("Language"))?)?;
                if chinese.is_none()
                    && language.split(';').any(|code| {
                        u32::from_str_radix(code.trim(), 16).is_ok_and(|id| id & 0x3ff == 4)
                    })
                {
                    chinese = Some(token.clone());
                }
                names.push(name);
            }
            if let Some(requested) = requested.as_ref() {
                let Some(token) = selected.as_ref() else {
                    bail!("找不到语音「{requested}」。可用语音：{}", names.join("、"));
                };
                voice.SetVoice(token)?;
            }
            Ok(Self {
                voice,
                chinese,
                default,
                explicit: requested.is_some(),
                paused: false,
                _apartment: apartment,
            })
        }
    }
}

impl Backend for SystemSpeech {
    fn speak(&mut self, text: &str, speed: u16) -> Result<()> {
        self.stop()?;
        unsafe {
            if !self.explicit {
                let needs_chinese = text
                    .chars()
                    .any(|c| matches!(c, '\u{3400}'..='\u{9fff}' | '\u{20000}'..='\u{323af}'));
                let token = if needs_chinese {
                    self.chinese.as_ref().context("没有可用的中文 SAPI 声音；请安装兼容的中文语音包，或通过 TLEGADO_TTS_VOICE 指定声音")?
                } else {
                    &self.default
                };
                self.voice.SetVoice(token)?;
            }
            self.rate(speed)?;
            // Text is always literal, never SSML, filenames, or shell code.
            self.voice.Speak(
                &HSTRING::from(text),
                (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_NOT_XML.0) as u32,
                None,
            )?;
        }
        Ok(())
    }
    fn pause(&mut self, paused: bool) -> Result<()> {
        if self.paused != paused {
            unsafe {
                if paused {
                    self.voice.Pause()?;
                } else {
                    self.voice.Resume()?;
                }
            }
            self.paused = paused;
        }
        Ok(())
    }
    fn rate(&mut self, speed: u16) -> Result<()> {
        // SAPI rate is logarithmic and engine-dependent; these are approximate multipliers.
        let rate = ((f64::from(speed) / 100.0).log2() * 10.0)
            .round()
            .clamp(-10.0, 10.0) as i32;
        unsafe {
            self.voice.SetRate(rate)?;
        }
        Ok(())
    }
    fn finished(&mut self) -> Result<bool> {
        let mut status = SPVOICESTATUS::default();
        unsafe {
            self.voice.GetStatus(&mut status, std::ptr::null_mut())?;
        }
        status
            .hrLastResult
            .ok()
            .context("语音输出失败，请检查默认音频设备")?;
        Ok(status.dwRunningState == SPRS_DONE.0 as u32)
    }
    fn stop(&mut self) -> Result<()> {
        unsafe {
            self.voice.Speak(
                PCWSTR::null(),
                (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32,
                None,
            )?;
        }
        self.pause(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "requires installed Chinese SAPI voice and plays a short audible sample"]
    fn windows_sapi_smoke() {
        let mut speech = SystemSpeech::new().unwrap();
        speech.speak("听书功能测试，暂停后继续。", 100).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        speech.pause(true).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!speech.finished().unwrap());
        speech.pause(false).unwrap();
        speech.rate(125).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !speech.finished().unwrap() {
            assert!(Instant::now() < deadline, "speech did not finish");
            std::thread::sleep(Duration::from_millis(20));
        }
        speech.speak("这段用于验证停止。", 100).unwrap();
        speech.pause(true).unwrap();
        speech.stop().unwrap();
    }
}
