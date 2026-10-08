//! Settings file behaviour, through the public interface only.

use markuli_core::{Config, Style};

#[test]
fn defaults_match_the_spec() {
    let config = Config::default();
    assert_eq!(config.toggle, "alt+Backquote");
    assert_eq!(config.clear, "alt+Digit1");
    assert!(!config.launch_at_login);
}

#[test]
fn saved_settings_load_back_unchanged() {
    let config = Config {
        toggle: "control+shift+KeyD".into(),
        clear: "F9".into(),
        launch_at_login: true,
        style: Style { color: 4, width: 0 },
    };
    assert_eq!(Config::parse(&config.to_text()), config);
}

#[test]
fn unknown_and_malformed_lines_are_ignored() {
    let text = "\u{feff}garbage\n=\n\n# comment\nfuture_key=1\ntoggle=alt+KeyQ\n===\nlaunch_at_login=maybe\nclear\n";
    let config = Config::parse(text);
    assert_eq!(config.toggle, "alt+KeyQ");
    assert_eq!(config.clear, Config::default().clear);
    assert!(!config.launch_at_login);
}

#[test]
fn empty_hotkey_values_keep_the_default() {
    let config = Config::parse("toggle=\nclear=   \n");
    assert_eq!(config, Config::default());
}

#[test]
fn whitespace_and_windows_line_endings_are_tolerated() {
    let config = Config::parse("  toggle =  alt+KeyQ \r\nlaunch_at_login=true\r\n");
    assert_eq!(config.toggle, "alt+KeyQ");
    assert!(config.launch_at_login);
}

#[test]
fn the_last_value_for_a_key_wins() {
    let config = Config::parse("toggle=alt+KeyA\ntoggle=alt+KeyB\n");
    assert_eq!(config.toggle, "alt+KeyB");
}

#[test]
fn arbitrary_bytes_never_panic() {
    for text in [
        "\0\0",
        "=\n=\n",
        "toggle",
        "toggle==",
        "\u{1F600}=\u{1F600}",
    ] {
        let _ = Config::parse(text);
    }
}

#[test]
fn the_default_style_is_red_and_medium() {
    assert_eq!(Config::default().style, Style { color: 1, width: 1 });
}

#[test]
fn style_keys_are_read_and_written() {
    let config = Config::parse("color=3\nwidth=2\n");
    assert_eq!(config.style, Style { color: 3, width: 2 });
    assert!(config.to_text().contains("color=3\n"));
    assert!(config.to_text().contains("width=2\n"));
}

#[test]
fn invalid_or_out_of_range_style_values_keep_the_defaults() {
    for text in [
        "color=5\nwidth=3",
        "color=-1\nwidth=x",
        "color=\nwidth= ",
        "color=1.5",
    ] {
        assert_eq!(Config::parse(text).style, Style::default(), "{text}");
    }
    let config = Config::parse("color=9\nwidth=2\n");
    assert_eq!(config.style, Style { color: 1, width: 2 });
}
