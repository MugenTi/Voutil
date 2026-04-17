use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use slint::platform::Key;
use slint::SharedString;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, PartialOrd, Ord)]
pub enum InputEvent {
    AlwaysOnTop,
    Fullscreen,
    OpenFile,
    NextImage,
    PreviousImage,
    ResetView,
    ZoomActualSize,
    Copy,
    Paste,
    CropSelection,
    ZenMode,
    PerfectFullscreen,
    ToggleThumbnails,
    Exit,
}

pub type Shortcuts = BTreeMap<InputEvent, Vec<SimultaneousKeypresses>>;

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize, PartialOrd, Ord)]
pub struct SimultaneousKeypresses {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl SimultaneousKeypresses {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Default::default()
        }
    }

    pub fn from_key(key: Key) -> Self {
        let s: SharedString = key.into();
        Self {
            key: s.to_string(),
            ..Default::default()
        }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn matches(&self, text: &str, ctrl: bool, alt: bool, shift: bool) -> bool {
        let mut key_match = if text.len() == 1 && self.key.len() == 1 {
            text.to_lowercase() == self.key.to_lowercase()
        } else {
            text == self.key
        };

        // Normalize Return key (\r vs \n)
        if !key_match && text.len() == 1 && self.key.len() == 1 {
            let t = text.as_bytes()[0];
            let k = self.key.as_bytes()[0];
            if (t == 10 || t == 13) && (k == 10 || k == 13) {
                key_match = true;
            }
        }

        key_match && self.ctrl == ctrl && self.alt == alt && self.shift == shift
    }
}

pub trait ShortcutExt {
    fn default_keys() -> Self;
}

impl ShortcutExt for Shortcuts {
    fn default_keys() -> Self {
        let mut s = Shortcuts::default();
        s.insert(InputEvent::OpenFile, vec![SimultaneousKeypresses::new("O").ctrl()]);
        s.insert(InputEvent::Fullscreen, vec![SimultaneousKeypresses::new("F")]);
        s.insert(InputEvent::AlwaysOnTop, vec![SimultaneousKeypresses::new("T").ctrl()]);
        s.insert(InputEvent::ToggleThumbnails, vec![SimultaneousKeypresses::new("T")]);
        s.insert(InputEvent::CropSelection, vec![SimultaneousKeypresses::new("Y").ctrl()]);
        s.insert(InputEvent::Exit, vec![
            SimultaneousKeypresses::from_key(Key::Escape),
            SimultaneousKeypresses::new("Q")
        ]);
        s.insert(InputEvent::PreviousImage, vec![
            SimultaneousKeypresses::new("A"),
            SimultaneousKeypresses::from_key(Key::LeftArrow),
            SimultaneousKeypresses::from_key(Key::LeftArrow).alt()
        ]);
        s.insert(InputEvent::NextImage, vec![
            SimultaneousKeypresses::new("D"),
            SimultaneousKeypresses::from_key(Key::RightArrow),
            SimultaneousKeypresses::from_key(Key::RightArrow).alt()
        ]);
        s.insert(InputEvent::ZoomActualSize, vec![SimultaneousKeypresses::new("1")]);
        s.insert(InputEvent::ResetView, vec![SimultaneousKeypresses::new("V")]);
        s.insert(InputEvent::Copy, vec![SimultaneousKeypresses::new("C").ctrl()]);
        s.insert(InputEvent::Paste, vec![SimultaneousKeypresses::new("V").ctrl()]);
        s.insert(InputEvent::ZenMode, vec![SimultaneousKeypresses::new("Z")]);
        s.insert(InputEvent::PerfectFullscreen, vec![SimultaneousKeypresses::from_key(Key::Return)]);
        s
    }
}

pub fn lookup(
    shortcuts: &Shortcuts,
    text: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Option<InputEvent> {
    for (input_event, key_list) in shortcuts {
        for keys in key_list {
            if keys.matches(text, ctrl, alt, shift) {
                return Some(input_event.clone());
            }
        }
    }
    None
}
