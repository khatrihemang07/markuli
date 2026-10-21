//! Settings file behaviour, through the public interface only.

use markuli_core::{Config, Palette, RecentColors, Style};

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
        toolbar: markuli_core::ToolbarPosition::Left,
        palette: Palette {
            colors: [
                [1, 2, 3],
                [255, 0, 128],
                [0, 0, 0],
                [16, 32, 48],
                [250, 250, 250],
            ],
            widths: [0.5, 7.5, 20.0],
        },
        recent: RecentColors::new(&[[9, 8, 7], [1, 1, 1]]),
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
    assert_eq!(Config::default().style, Style { color: 0, width: 1 });
}

#[test]
fn style_keys_are_read_and_written() {
    let config = Config::parse("palette=3\ncolor=3\nwidth=2\n");
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
        let text = format!("palette=3\n{text}");
        assert_eq!(Config::parse(&text).style, Style::default(), "{text}");
    }
    let config = Config::parse("palette=3\ncolor=9\nwidth=2\n");
    assert_eq!(config.style, Style { color: 0, width: 2 });
}

#[test]
fn the_toolbar_position_is_read_tolerantly() {
    use markuli_core::ToolbarPosition as Pos;
    assert_eq!(Config::default().toolbar, Pos::Top);
    for (text, want) in [
        ("toolbar=left\n", Pos::Left),
        ("toolbar=right\n", Pos::Right),
        ("toolbar=bottom\n", Pos::Bottom),
        ("toolbar = bottom \n", Pos::Bottom),
        ("toolbar=sideways\n", Pos::Top),
        ("toolbar=\n", Pos::Top),
        ("toolbar=LEFT\n", Pos::Top),
    ] {
        assert_eq!(Config::parse(text).toolbar, want, "{text:?}");
    }
    let c = Config {
        toolbar: Pos::Right,
        ..Config::default()
    };
    assert_eq!(Config::parse(&c.to_text()), c);
}

#[test]
fn the_default_palette_is_red_amber_green_blue_grape_at_2_4_8_pt() {
    assert_eq!(Config::default().palette, Palette::DEFAULT);
    let text = Config::default().to_text();
    assert!(
        text.contains("colors=#e03131,#f59e0b,#16a34a,#007aff,#be4bdb\n"),
        "{text}"
    );
    assert!(text.contains("widths=2,4,8\n"), "{text}");
    assert!(text.contains("palette=3\n"), "{text}");
}

#[test]
fn palette_values_are_read_slot_by_slot() {
    let c = Config::parse(
        "palette=3\ncolors=#112233,#445566,#778899,#aabbcc,#ddeeff\nwidths=0.5,3.25,30\n",
    );
    assert_eq!(c.palette.colors[0], [0x11, 0x22, 0x33]);
    assert_eq!(c.palette.colors[4], [0xdd, 0xee, 0xff]);
    assert_eq!(c.palette.widths, [0.5, 3.25, 30.0]);
}

#[test]
fn invalid_palette_slots_fall_back_to_their_default_alone() {
    let d = Palette::DEFAULT;
    let c =
        Config::parse("palette=3\ncolors=#112233,red,#12345,#GGGGGG,#AABBCC\nwidths=abc,5,NaN\n");
    assert_eq!(c.palette.colors[0], [0x11, 0x22, 0x33]);
    assert_eq!(c.palette.colors[1], d.colors[1]);
    assert_eq!(c.palette.colors[2], d.colors[2]);
    assert_eq!(c.palette.colors[3], d.colors[3]);
    assert_eq!(c.palette.colors[4], [0xaa, 0xbb, 0xcc]);
    assert_eq!(c.palette.widths, [d.widths[0], 5.0, d.widths[2]]);
}

#[test]
fn short_long_and_odd_palette_lists_are_tolerated() {
    let d = Palette::DEFAULT;
    let c = Config::parse("palette=3\ncolors=#010203\nwidths=9,,\n");
    assert_eq!(c.palette.colors[0], [1, 2, 3]);
    assert_eq!(c.palette.colors[1..], d.colors[1..]);
    assert!((c.palette.widths[0] - 9.0).abs() < f32::EPSILON);
    assert_eq!(c.palette.widths[1..], d.widths[1..]);
    let c = Config::parse(
        "palette=3\ncolors=#010203,#010203,#010203,#010203,#010203,#ffffff,#ffffff\nwidths=1,1,1,1,1\n",
    );
    assert_eq!(c.palette.colors[4], [1, 2, 3]);
    assert_eq!(Config::parse("palette=3\ncolors=\nwidths=").palette, d);
}

#[test]
fn out_of_range_widths_are_clamped_and_snapped() {
    let c = Config::parse("palette=3\nwidths=0.1,2.3,500\n");
    assert_eq!(c.palette.widths, [0.5, 2.25, 30.0]);
}

#[test]
fn a_0_3_0_config_without_the_version_key_resets_the_palette_and_style_once() {
    let old = "toggle=control+KeyD\nclear=F9\nlaunch_at_login=true\ncolor=3\nwidth=2\n\
               toolbar=left\ncolors=#112233,#445566,#778899,#aabbcc,#ddeeff\nwidths=1,2,4\n";
    let c = Config::parse(old);
    assert_eq!(c.palette, Palette::DEFAULT);
    assert_eq!(c.style, Style::default());
    assert_eq!(c.toggle, "control+KeyD");
    assert_eq!(c.clear, "F9");
    assert!(c.launch_at_login);
    assert_eq!(c.toolbar, markuli_core::ToolbarPosition::Left);
    // Saved once, the new format is kept.
    let saved = Config::parse(&c.to_text());
    assert_eq!(saved, c);
}

#[test]
fn a_current_config_round_trips_and_an_unknown_version_resets() {
    let mut c = Config::default();
    c.palette.widths = [0.75, 12.25, 30.0];
    c.palette.colors[2] = [9, 8, 7];
    c.style = Style { color: 2, width: 0 };
    assert_eq!(Config::parse(&c.to_text()), c);
    let text = c.to_text().replace("palette=3", "palette=4");
    assert_eq!(Config::parse(&text).palette, Palette::DEFAULT);
}

const V2_DEFAULTS: &str = "#e03131,#1971c2,#2f9e44,#fab005,#1e1e1e";

#[test]
fn a_version_2_config_with_the_old_defaults_gets_the_new_ones() {
    let text = format!("palette=2\ncolor=3\nwidth=2\ncolors={V2_DEFAULTS}\nwidths=2,4,8\n");
    let c = Config::parse(&text);
    assert_eq!(c.palette, Palette::DEFAULT);
    assert_eq!(c.style, Style { color: 3, width: 2 }, "the Style is kept");
}

#[test]
fn a_version_2_config_keeps_the_slots_the_user_changed() {
    let text = "palette=2\ncolor=1\nwidth=0\ntoolbar=left\n\
                colors=#e03131,#123456,#2f9e44,#fab005,#abcdef\nwidths=1.5,4,9\n";
    let c = Config::parse(text);
    let d = Palette::DEFAULT;
    assert_eq!(c.palette.colors[0], d.colors[0]);
    assert_eq!(c.palette.colors[1], [0x12, 0x34, 0x56], "customized, kept");
    assert_eq!(c.palette.colors[2], d.colors[2]);
    assert_eq!(c.palette.colors[3], d.colors[3]);
    assert_eq!(c.palette.colors[4], [0xab, 0xcd, 0xef], "customized, kept");
    assert_eq!(c.palette.widths, [1.5, 4.0, 9.0]);
    assert_eq!(c.style, Style { color: 1, width: 0 });
    assert_eq!(c.toolbar, markuli_core::ToolbarPosition::Left);
    // Saved once, it is a version 3 file.
    assert!(c.to_text().contains("palette=3\n"));
}

#[test]
fn a_version_1_config_still_resets_everything() {
    let text = format!("palette=1\ncolor=3\ncolors={V2_DEFAULTS}\nwidths=5,5,5\n");
    let c = Config::parse(&text);
    assert_eq!(c.palette, Palette::DEFAULT);
    assert_eq!(c.style, Style::default());
}

#[test]
fn recent_colors_round_trip_and_default_to_none() {
    assert_eq!(Config::default().recent, RecentColors::default());
    let text = Config::default().to_text();
    assert!(text.contains("recent=\n"), "{text}");
    let c = Config {
        recent: RecentColors::new(&[[1, 2, 3], [255, 0, 16]]),
        ..Config::default()
    };
    let text = c.to_text();
    assert!(text.contains("recent=#010203,#ff0010\n"), "{text}");
    assert_eq!(Config::parse(&text).recent, c.recent);
}

#[test]
fn bad_recent_items_are_skipped_and_the_list_is_capped_at_six() {
    let c = Config::parse(
        "palette=3\nrecent=#010203,red,#12345,,#040506,#GGGGGG,#070809,#0a0b0c,#0d0e0f,#101112,#131415\n",
    );
    assert_eq!(
        c.recent.as_slice(),
        [
            [1, 2, 3],
            [4, 5, 6],
            [7, 8, 9],
            [10, 11, 12],
            [13, 14, 15],
            [16, 17, 18]
        ]
    );
    assert_eq!(
        Config::parse(
            "palette=3
recent=nonsense
"
        )
        .recent,
        RecentColors::default()
    );
}

#[test]
fn recent_colors_survive_a_palette_reset_of_an_old_file() {
    // The list is not Palette data: an old file without it simply has none.
    assert_eq!(Config::parse("color=3\n").recent, RecentColors::default());
    assert_eq!(
        Config::parse("recent=#010203\n").recent.as_slice(),
        [[1, 2, 3]]
    );
}
