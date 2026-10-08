//! The Toggle and Clear hotkeys: which keys, registering them with the OS,
//! rebinding at runtime, and turning a recorded key press into hotkey text.
//!
//! Hotkeys are stored as text like `alt+Backquote` (the format of
//! `global_hotkey::hotkey::HotKey`). Key names are the W3C `KeyboardEvent.code`
//! names, so they do not change with the keyboard layout.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;
use markuli_core::Config;
use std::str::FromStr;

/// One recordable key: its name, how it reads in the UI, and its native codes.
struct Key {
    name: &'static str,
    label: &'static str,
    /// macOS virtual key code (ANSI layout positions; they are physical keys).
    mac: u16,
    /// Windows virtual-key code.
    vk: u16,
}

macro_rules! keys {
    ($(($name:literal, $label:literal, $mac:literal, $vk:literal)),* $(,)?) => {
        &[$(Key { name: $name, label: $label, mac: $mac, vk: $vk }),*]
    };
}

#[rustfmt::skip]
const KEYS: &[Key] = keys![
    ("KeyA","A",0,0x41), ("KeyB","B",11,0x42), ("KeyC","C",8,0x43), ("KeyD","D",2,0x44),
    ("KeyE","E",14,0x45), ("KeyF","F",3,0x46), ("KeyG","G",5,0x47), ("KeyH","H",4,0x48),
    ("KeyI","I",34,0x49), ("KeyJ","J",38,0x4A), ("KeyK","K",40,0x4B), ("KeyL","L",37,0x4C),
    ("KeyM","M",46,0x4D), ("KeyN","N",45,0x4E), ("KeyO","O",31,0x4F), ("KeyP","P",35,0x50),
    ("KeyQ","Q",12,0x51), ("KeyR","R",15,0x52), ("KeyS","S",1,0x53), ("KeyT","T",17,0x54),
    ("KeyU","U",32,0x55), ("KeyV","V",9,0x56), ("KeyW","W",13,0x57), ("KeyX","X",7,0x58),
    ("KeyY","Y",16,0x59), ("KeyZ","Z",6,0x5A),
    ("Digit0","0",29,0x30), ("Digit1","1",18,0x31), ("Digit2","2",19,0x32), ("Digit3","3",20,0x33),
    ("Digit4","4",21,0x34), ("Digit5","5",23,0x35), ("Digit6","6",22,0x36), ("Digit7","7",26,0x37),
    ("Digit8","8",28,0x38), ("Digit9","9",25,0x39),
    ("Backquote","`",50,0xC0), ("Minus","-",27,0xBD), ("Equal","=",24,0xBB),
    ("BracketLeft","[",33,0xDB), ("BracketRight","]",30,0xDD), ("Backslash","\\",42,0xDC),
    ("Semicolon",";",41,0xBA), ("Quote","'",39,0xDE), ("Comma",",",43,0xBC),
    ("Period",".",47,0xBE), ("Slash","/",44,0xBF),
    ("Space","Space",49,0x20), ("Tab","Tab",48,0x09), ("Enter","Enter",36,0x0D),
    ("Backspace","Backspace",51,0x08),
    ("ArrowLeft","Left",123,0x25), ("ArrowUp","Up",126,0x26),
    ("ArrowRight","Right",124,0x27), ("ArrowDown","Down",125,0x28),
    ("F1","F1",122,0x70), ("F2","F2",120,0x71), ("F3","F3",99,0x72), ("F4","F4",118,0x73),
    ("F5","F5",96,0x74), ("F6","F6",97,0x75), ("F7","F7",98,0x76), ("F8","F8",100,0x77),
    ("F9","F9",101,0x78), ("F10","F10",109,0x79), ("F11","F11",103,0x7A), ("F12","F12",111,0x7B),
];

/// Which modifier keys were held when a key was pressed.
#[allow(
    clippy::struct_excessive_bools,
    reason = "the four modifier keys are independent flags"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Command on macOS, the Windows key on Windows.
    pub logo: bool,
}

/// Name of the key at a macOS virtual key code.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "macOS recorder only")
)]
pub fn key_from_mac(code: u16) -> Option<&'static str> {
    KEYS.iter().find(|k| k.mac == code).map(|k| k.name)
}

/// Name of the key at a Windows virtual-key code.
#[cfg_attr(
    not(target_os = "windows"),
    allow(dead_code, reason = "Windows recorder only")
)]
pub fn key_from_vk(vk: u16) -> Option<&'static str> {
    KEYS.iter().find(|k| k.vk == vk).map(|k| k.name)
}

/// Turns a key press into hotkey text, or `None` if it can't be a global
/// hotkey. A bare letter, digit or symbol would steal normal typing from
/// every app, so those need a modifier. Function keys may stand alone.
pub fn recorded(mods: Mods, key: &str) -> Option<String> {
    let function_key =
        key.len() >= 2 && key.starts_with('F') && key[1..].bytes().all(|b| b.is_ascii_digit());
    if !(mods.ctrl || mods.alt || mods.shift || mods.logo || function_key) {
        return None;
    }
    let mut flags = Modifiers::empty();
    flags.set(Modifiers::CONTROL, mods.ctrl);
    flags.set(Modifiers::ALT, mods.alt);
    flags.set(Modifiers::SHIFT, mods.shift);
    flags.set(Modifiers::SUPER, mods.logo);
    let code = Code::from_str(key).ok()?;
    KEYS.iter().find(|k| k.name == key)?;
    Some(HotKey::new(Some(flags), code).into_string())
}

/// Human-readable form, e.g. `alt+Backquote` becomes Alt plus the backquote key.
pub fn label(text: &str, logo_name: &str) -> String {
    let Ok(hotkey) = HotKey::from_str(text) else {
        return text.to_owned();
    };
    let mut out = String::new();
    for (flag, name) in [
        (Modifiers::CONTROL, "Ctrl"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, logo_name),
    ] {
        if hotkey.mods.contains(flag) {
            out.push_str(name);
            out.push('+');
        }
    }
    let key = hotkey.key.to_string();
    let key = key.as_str();
    out.push_str(KEYS.iter().find(|k| k.name == key).map_or(key, |k| k.label));
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Toggle = 0,
    Clear = 1,
}

/// Why a rebind did not happen. The old binding stays in every case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RebindError {
    /// The text is not a hotkey.
    Invalid,
    /// The other Markuli hotkey already uses this combo.
    UsedByOther(Binding),
    /// The OS refused: another app owns the combo.
    Taken,
}

/// The OS side of registering a global hotkey; faked in tests.
pub trait Registrar {
    fn register(&self, hotkey: HotKey) -> bool;
    fn unregister(&self, hotkey: HotKey);
}

impl Registrar for GlobalHotKeyManager {
    fn register(&self, hotkey: HotKey) -> bool {
        GlobalHotKeyManager::register(self, hotkey).is_ok()
    }
    fn unregister(&self, hotkey: HotKey) {
        let _ = GlobalHotKeyManager::unregister(self, hotkey);
    }
}

pub struct Hotkeys<R: Registrar> {
    registrar: R,
    toggle: HotKey,
    clear: HotKey,
    /// The Settings recorder is listening: our own combos are released so the
    /// OS does not swallow them before the window sees the key press.
    suspended: bool,
}

impl<R: Registrar> Hotkeys<R> {
    /// Registers the configured hotkeys. A combo that is unparsable falls back
    /// to its default; one the OS refuses stays unregistered (the tray still
    /// works, and Settings lets the user pick another).
    pub fn new(registrar: R, config: &Config) -> Self {
        let defaults = Config::default();
        let pick = |text: &str, fallback: &str| {
            HotKey::from_str(text)
                .or_else(|_| HotKey::from_str(fallback))
                .unwrap_or_else(|_| HotKey::new(Some(Modifiers::ALT), Code::Backquote))
        };
        let toggle = pick(&config.toggle, &defaults.toggle);
        let mut clear = pick(&config.clear, &defaults.clear);
        if clear.id() == toggle.id() {
            clear = pick(&defaults.clear, &defaults.clear);
        }
        for hotkey in [toggle, clear] {
            if !registrar.register(hotkey) {
                eprintln!("markuli: {hotkey} is in use by another app");
            }
        }
        Self {
            registrar,
            toggle,
            clear,
            suspended: false,
        }
    }

    /// Releases both combos while the Settings recorder listens, so pressing
    /// one of Markuli's own combos reaches the recorder instead of firing it.
    pub fn suspend(&mut self) {
        if !self.suspended {
            self.suspended = true;
            self.registrar.unregister(self.toggle);
            self.registrar.unregister(self.clear);
        }
    }

    /// Registers both combos again (the recorder finished, was cancelled, or
    /// its window closed).
    pub fn resume(&mut self) {
        if self.suspended {
            self.suspended = false;
            for hotkey in [self.toggle, self.clear] {
                if !self.registrar.register(hotkey) {
                    eprintln!("markuli: {hotkey} is in use by another app");
                }
            }
        }
    }

    /// Which binding a hotkey event id belongs to.
    pub fn binding_for(&self, id: u32) -> Option<Binding> {
        if id == self.toggle.id() {
            Some(Binding::Toggle)
        } else if id == self.clear.id() {
            Some(Binding::Clear)
        } else {
            None
        }
    }

    /// Current combo of a binding, as text for the Settings file.
    pub fn text(&self, binding: Binding) -> String {
        match binding {
            Binding::Toggle => self.toggle,
            Binding::Clear => self.clear,
        }
        .into_string()
    }

    /// Moves a binding to a new combo immediately. The new combo is registered
    /// before the old one is released, so a refusal leaves everything as it was.
    pub fn rebind(&mut self, binding: Binding, text: &str) -> Result<(), RebindError> {
        let new = HotKey::from_str(text).map_err(|_| RebindError::Invalid)?;
        let (slot, other, other_binding) = match binding {
            Binding::Toggle => (&mut self.toggle, self.clear, Binding::Clear),
            Binding::Clear => (&mut self.clear, self.toggle, Binding::Toggle),
        };
        if new.id() == other.id() {
            return Err(RebindError::UsedByOther(other_binding));
        }
        if new.id() == slot.id() {
            return Ok(());
        }
        if !self.registrar.register(new) {
            return Err(RebindError::Taken);
        }
        if self.suspended {
            // Only probing: `resume` registers whatever the bindings are then.
            self.registrar.unregister(new);
        } else {
            self.registrar.unregister(*slot);
        }
        *slot = new;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Fake OS: owns a set of combos and refuses those held by "another app".
    #[derive(Default)]
    struct Fake {
        taken_elsewhere: Vec<u32>,
        registered: RefCell<Vec<u32>>,
    }

    impl Registrar for &Fake {
        fn register(&self, hotkey: HotKey) -> bool {
            if self.taken_elsewhere.contains(&hotkey.id()) {
                return false;
            }
            self.registered.borrow_mut().push(hotkey.id());
            true
        }
        fn unregister(&self, hotkey: HotKey) {
            self.registered.borrow_mut().retain(|id| *id != hotkey.id());
        }
    }

    fn id(text: &str) -> u32 {
        HotKey::from_str(text).unwrap().id()
    }

    #[test]
    fn defaults_register_both_hotkeys() {
        let os = Fake::default();
        let hotkeys = Hotkeys::new(&os, &Config::default());
        assert_eq!(
            hotkeys.binding_for(id("alt+Backquote")),
            Some(Binding::Toggle)
        );
        assert_eq!(hotkeys.binding_for(id("alt+Digit1")), Some(Binding::Clear));
        assert_eq!(os.registered.borrow().len(), 2);
    }

    #[test]
    fn rebinding_takes_effect_and_releases_the_old_combo() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        assert_eq!(hotkeys.rebind(Binding::Toggle, "control+KeyD"), Ok(()));
        assert_eq!(
            hotkeys.binding_for(id("control+KeyD")),
            Some(Binding::Toggle)
        );
        assert_eq!(hotkeys.binding_for(id("alt+Backquote")), None);
        assert!(!os.registered.borrow().contains(&id("alt+Backquote")));
        assert_eq!(hotkeys.text(Binding::Toggle), "control+KeyD");
    }

    #[test]
    fn a_combo_owned_by_another_app_keeps_the_old_binding() {
        let os = Fake {
            taken_elsewhere: vec![id("control+KeyD")],
            ..Fake::default()
        };
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        assert_eq!(
            hotkeys.rebind(Binding::Toggle, "control+KeyD"),
            Err(RebindError::Taken)
        );
        assert_eq!(
            hotkeys.binding_for(id("alt+Backquote")),
            Some(Binding::Toggle)
        );
        assert!(os.registered.borrow().contains(&id("alt+Backquote")));
    }

    #[test]
    fn reusing_the_other_markuli_hotkey_is_refused() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        assert_eq!(
            hotkeys.rebind(Binding::Toggle, "alt+Digit1"),
            Err(RebindError::UsedByOther(Binding::Clear))
        );
        assert_eq!(hotkeys.text(Binding::Toggle), "alt+Backquote");
    }

    #[test]
    fn rebinding_to_the_current_combo_does_nothing() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        assert_eq!(hotkeys.rebind(Binding::Clear, "alt+Digit1"), Ok(()));
        assert_eq!(os.registered.borrow().len(), 2);
    }

    #[test]
    fn listening_releases_both_combos_and_resuming_restores_them() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        hotkeys.suspend();
        assert!(os.registered.borrow().is_empty());
        hotkeys.resume();
        assert_eq!(os.registered.borrow().len(), 2);
        assert!(os.registered.borrow().contains(&id("alt+Backquote")));
        assert!(os.registered.borrow().contains(&id("alt+Digit1")));
    }

    #[test]
    fn while_listening_a_duplicate_of_the_other_binding_is_still_reported() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        hotkeys.suspend();
        assert_eq!(
            hotkeys.rebind(Binding::Toggle, "alt+Digit1"),
            Err(RebindError::UsedByOther(Binding::Clear))
        );
        hotkeys.resume();
        assert_eq!(hotkeys.text(Binding::Toggle), "alt+Backquote");
    }

    #[test]
    fn while_listening_a_combo_of_another_app_is_still_refused() {
        let os = Fake {
            taken_elsewhere: vec![id("control+KeyD")],
            ..Fake::default()
        };
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        hotkeys.suspend();
        assert_eq!(
            hotkeys.rebind(Binding::Toggle, "control+KeyD"),
            Err(RebindError::Taken)
        );
        hotkeys.resume();
        assert!(os.registered.borrow().contains(&id("alt+Backquote")));
    }

    #[test]
    fn a_rebind_while_listening_takes_effect_on_resume() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        hotkeys.suspend();
        assert_eq!(hotkeys.rebind(Binding::Toggle, "control+KeyD"), Ok(()));
        assert!(os.registered.borrow().is_empty());
        hotkeys.resume();
        let registered = os.registered.borrow();
        assert_eq!(registered.len(), 2);
        assert!(registered.contains(&id("control+KeyD")));
        assert!(!registered.contains(&id("alt+Backquote")));
    }

    #[test]
    fn garbage_text_is_invalid() {
        let os = Fake::default();
        let mut hotkeys = Hotkeys::new(&os, &Config::default());
        assert_eq!(
            hotkeys.rebind(Binding::Clear, "banana"),
            Err(RebindError::Invalid)
        );
    }

    #[test]
    fn bad_config_falls_back_to_defaults() {
        let os = Fake::default();
        let config = Config {
            toggle: "nonsense".into(),
            clear: "nonsense".into(),
            launch_at_login: false,
        };
        let hotkeys = Hotkeys::new(&os, &config);
        assert_eq!(hotkeys.text(Binding::Toggle), "alt+Backquote");
        assert_eq!(hotkeys.text(Binding::Clear), "alt+Digit1");
    }

    #[test]
    fn both_bindings_configured_alike_get_distinct_combos() {
        let os = Fake::default();
        let config = Config {
            toggle: "alt+KeyQ".into(),
            clear: "alt+KeyQ".into(),
            launch_at_login: false,
        };
        let hotkeys = Hotkeys::new(&os, &config);
        assert_eq!(hotkeys.text(Binding::Toggle), "alt+KeyQ");
        assert_eq!(hotkeys.text(Binding::Clear), "alt+Digit1");
    }

    #[test]
    fn recording_alt_backquote_gives_the_default_toggle_text() {
        let mods = Mods {
            alt: true,
            ..Mods::default()
        };
        assert_eq!(
            recorded(mods, "Backquote").as_deref(),
            Some("alt+Backquote")
        );
    }

    #[test]
    fn recording_orders_modifiers_canonically() {
        let mods = Mods {
            ctrl: true,
            alt: true,
            shift: true,
            logo: false,
        };
        assert_eq!(
            recorded(mods, "KeyD").as_deref(),
            Some("shift+control+alt+KeyD")
        );
    }

    #[test]
    fn a_bare_letter_cannot_be_a_global_hotkey() {
        assert_eq!(recorded(Mods::default(), "KeyA"), None);
    }

    #[test]
    fn a_function_key_may_stand_alone() {
        assert_eq!(recorded(Mods::default(), "F9").as_deref(), Some("F9"));
    }

    #[test]
    fn unknown_key_names_are_not_recordable() {
        let mods = Mods {
            alt: true,
            ..Mods::default()
        };
        assert_eq!(recorded(mods, "Escape"), None);
        assert_eq!(recorded(mods, "Fnord"), None);
    }

    #[test]
    fn native_key_codes_map_to_names() {
        assert_eq!(key_from_mac(50), Some("Backquote"));
        assert_eq!(key_from_vk(0xC0), Some("Backquote"));
        assert_eq!(key_from_mac(53), None); // Esc is reserved for cancelling
        assert_eq!(key_from_vk(0x1B), None);
    }

    #[test]
    fn every_listed_key_round_trips_through_the_hotkey_parser() {
        for key in KEYS {
            let mods = Mods {
                alt: true,
                ..Mods::default()
            };
            let text =
                recorded(mods, key.name).unwrap_or_else(|| panic!("{} not recordable", key.name));
            assert!(HotKey::from_str(&text).is_ok(), "{text}");
        }
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(label("alt+Backquote", "Cmd"), "Alt+`");
        assert_eq!(label("shift+control+super+KeyD", "Cmd"), "Ctrl+Shift+Cmd+D");
        assert_eq!(label("F9", "Cmd"), "F9");
    }
}
