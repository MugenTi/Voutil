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
        Self {
            key: slint_to_human_readable(SharedString::from(key).as_str()),
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
        let stored_key = human_readable_to_slint(&self.key);
        
        let mut key_match = if text.len() == 1 && stored_key.len() == 1 {
            text.to_lowercase() == stored_key.to_lowercase()
        } else {
            text == stored_key
        };

        // Normalize Return key (\r vs \n)
        if !key_match && text.len() == 1 && stored_key.len() == 1 {
            let t = text.as_bytes()[0];
            let k = stored_key.as_bytes()[0];
            if (t == 10 || t == 13) && (k == 10 || k == 13) {
                key_match = true;
            }
        }

        key_match && self.ctrl == ctrl && self.alt == alt && self.shift == shift
    }
}

pub fn slint_to_human_readable(key: &str) -> String {
    match key {
        "\r" | "\n" => "Return".to_string(),
        "\u{001b}" => "Esc".to_string(),
        "\u{007f}" => "Delete".to_string(),
        "\u{0008}" => "Backspace".to_string(),
        "\t" => "Tab".to_string(),
        " " => "Space".to_string(),
        "\u{f702}" | "\u{f060}" => "Left".to_string(),
        "\u{f703}" | "\u{f061}" => "Right".to_string(),
        "\u{f700}" | "\u{f062}" => "Up".to_string(),
        "\u{f701}" | "\u{f063}" => "Down".to_string(),
        _ => key.to_string(),
    }
}

pub fn human_readable_to_slint(name: &str) -> String {
    match name {
        "Return" | "Enter" => "\r".to_string(),
        "Esc" | "Escape" => "\u{001b}".to_string(),
        "Delete" | "Del" => "\u{007f}".to_string(),
        "Backspace" => "\u{0008}".to_string(),
        "Tab" => "\t".to_string(),
        "Space" => " ".to_string(),
        "Left" => "\u{f702}".to_string(),
        "Right" => "\u{f703}".to_string(),
        "Up" => "\u{f700}".to_string(),
        "Down" => "\u{f701}".to_string(),
        _ => name.to_string(),
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
            SimultaneousKeypresses::new("Esc"),
            SimultaneousKeypresses::new("Q")
        ]);
        s.insert(InputEvent::PreviousImage, vec![
            SimultaneousKeypresses::new("A"),
            SimultaneousKeypresses::new("Left")
        ]);
        s.insert(InputEvent::NextImage, vec![
            SimultaneousKeypresses::new("D"),
            SimultaneousKeypresses::new("Right")
        ]);
        s.insert(InputEvent::ZoomActualSize, vec![SimultaneousKeypresses::new("1")]);
        s.insert(InputEvent::ResetView, vec![SimultaneousKeypresses::new("V")]);
        s.insert(InputEvent::Copy, vec![SimultaneousKeypresses::new("C").ctrl()]);
        s.insert(InputEvent::Paste, vec![SimultaneousKeypresses::new("V").ctrl()]);
        s.insert(InputEvent::ZenMode, vec![SimultaneousKeypresses::new("Z")]);
        s.insert(InputEvent::PerfectFullscreen, vec![SimultaneousKeypresses::new("Return")]);
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
