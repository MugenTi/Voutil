use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

pub type Shortcuts = BTreeMap<InputEvent, SimultaneousKeypresses>;

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
        let key_match = if text.len() == 1 && self.key.len() == 1 {
            text.to_lowercase() == self.key.to_lowercase()
        } else {
            text == self.key
        };

        key_match && self.ctrl == ctrl && self.alt == alt && self.shift == shift
    }
}

pub trait ShortcutExt {
    fn default_keys() -> Self;
}

impl ShortcutExt for Shortcuts {
    fn default_keys() -> Self {
        let mut s = Shortcuts::default();
        s.insert(InputEvent::OpenFile, SimultaneousKeypresses::new("O").ctrl());
        s.insert(InputEvent::Fullscreen, SimultaneousKeypresses::new("F"));
        s.insert(InputEvent::AlwaysOnTop, SimultaneousKeypresses::new("T").ctrl());
        s.insert(InputEvent::ToggleThumbnails, SimultaneousKeypresses::new("T"));
        s.insert(InputEvent::CropSelection, SimultaneousKeypresses::new("Y").ctrl());
        s.insert(InputEvent::Exit, SimultaneousKeypresses::new("\u{001b}")); // Escape
        s.insert(InputEvent::PreviousImage, SimultaneousKeypresses::new("A"));
        s.insert(InputEvent::NextImage, SimultaneousKeypresses::new("D"));
        s.insert(InputEvent::ZoomActualSize, SimultaneousKeypresses::new("1"));
        s.insert(InputEvent::ResetView, SimultaneousKeypresses::new("V"));
        s.insert(InputEvent::Copy, SimultaneousKeypresses::new("C").ctrl());
        s.insert(InputEvent::Paste, SimultaneousKeypresses::new("V").ctrl());
        s.insert(InputEvent::ZenMode, SimultaneousKeypresses::new("Z"));
        s.insert(InputEvent::PerfectFullscreen, SimultaneousKeypresses::new("\r")); // Return
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
    for (input_event, keys) in shortcuts {
        if keys.matches(text, ctrl, alt, shift) {
            return Some(input_event.clone());
        }
    }
    
    // Fallback for special keys that might not match exactly or have multiple defaults
    if ctrl || alt || shift {
        return None;
    }

    if text == "\u{0011}" { // LeftArrow in some contexts? No, Slint uses special strings.
        // Actually Slint's Key.LeftArrow is a specific string.
        // I'll handle them by their string values if they are known.
    }
    
    // Slint Key constants as strings
    if text == "\u{f060}" { return Some(InputEvent::PreviousImage); } // Left Arrow
    if text == "\u{f061}" { return Some(InputEvent::NextImage); }     // Right Arrow

    None
}
