//! Settings: the two hotkeys and launch at login. Never Ink.

/// What the user can change in Settings. Hotkeys are kept as text because the
/// core knows no key codes; the platform layer parses and validates them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub toggle: String,
    pub clear: String,
    pub launch_at_login: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            toggle: "alt+Backquote".into(),
            clear: "alt+Digit1".into(),
            launch_at_login: false,
        }
    }
}

impl Config {
    /// Reads a `key=value` file. Unknown keys, malformed lines and empty
    /// values are skipped, so a damaged file never blocks startup.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "toggle" if !value.is_empty() => config.toggle = value.into(),
                "clear" if !value.is_empty() => config.clear = value.into(),
                "launch_at_login" => config.launch_at_login = value == "true",
                _ => {}
            }
        }
        config
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "toggle={}\nclear={}\nlaunch_at_login={}\n",
            self.toggle, self.clear, self.launch_at_login
        )
    }
}
