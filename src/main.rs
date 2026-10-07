//! Markuli: tray app that draws Ink over the display under the cursor.
//!
//! This file is the event-loop glue: OS events become core events, core
//! `View` state becomes window calls. Logic belongs in `markuli-core`.

mod hotkeys;
mod platform;
mod settings;

use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use hotkeys::{Binding, Hotkeys, RebindError};
use markuli_core::{Annotator, Config, Cursor, DisplayId, Event, Key, Point, Theme, View};
use platform::{Presenter, SettingsWindow};
use settings::SettingsEvent;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::monitor::MonitorHandle;
use winit::window::{CursorIcon, Theme as OsTheme, Window, WindowId};

/// Wake-ups sent from the hotkey and menu callbacks, so the loop never polls.
enum UserEvent {
    Hotkey(GlobalHotKeyEvent),
    Menu(MenuEvent),
    Settings(SettingsEvent),
}

/// The Overlay window and what it draws through.
struct Overlay {
    display: DisplayId,
    window: Window,
    presenter: Presenter,
}

struct App {
    core: Annotator,
    overlay: Option<Overlay>,
    cursor: Point,
    cursor_shape: Cursor,
    hotkeys: Hotkeys<GlobalHotKeyManager>,
    config: Config,
    /// Exists only while the Settings window is open.
    settings: Option<SettingsWindow>,
    settings_id: Option<MenuId>,
    /// Test hook: reopen Settings this many more times after it closes.
    reopen: u32,
    modifiers: ModifiersState,
    quit_id: Option<MenuId>,
    tray: Option<TrayIcon>,
}

impl App {
    fn on_toggle(&mut self, event_loop: &ActiveEventLoop) {
        let Some(monitor) = platform::monitor_under_cursor(event_loop)
            .or_else(|| event_loop.primary_monitor())
            .or_else(|| event_loop.available_monitors().next())
        else {
            return;
        };
        let display = display_id(&monitor);
        let view = self.core.handle(Event::ToggleDrawMode(display));
        let moved = self.overlay.as_ref().is_some_and(|o| o.display != display);
        if view.draw_mode && moved {
            // ADR-0002: one Overlay, recreated on the new display (Ink was Cleared).
            self.overlay = None;
        }
        self.sync(event_loop, view, Some(&monitor));
    }

    /// Brings the window in line with what the core says.
    fn sync(&mut self, event_loop: &ActiveEventLoop, view: View, monitor: Option<&MonitorHandle>) {
        if !view.overlay_needed {
            if self.overlay.take().is_some() {
                platform::release_memory();
            }
            return;
        }
        if self.overlay.is_none() {
            let Some(monitor) = monitor else { return };
            self.overlay = Some(create_overlay(event_loop, monitor));
            self.core.handle(Event::ScaleFactor(scale_of(monitor)));
            self.send_surface();
            self.core.handle(Event::SurfaceReset);
        }
        let Some(overlay) = self.overlay.as_ref() else {
            return;
        };
        overlay
            .presenter
            .set_click_through(&overlay.window, view.click_through());
        if view.draw_mode {
            overlay.window.set_cursor(cursor_icon(view.cursor));
            overlay.window.focus_window();
        }
        // First frame is painted before the window shows: no flash.
        self.redraw();
        if let Some(overlay) = self.overlay.as_ref() {
            overlay.window.set_visible(true);
        }
    }

    /// Tells the core the new Overlay's size and the OS theme. winit reads the
    /// theme from `effectiveAppearance` on macOS and from the
    /// `AppsUseLightTheme` registry value on Windows.
    fn send_surface(&mut self) {
        let Some(overlay) = self.overlay.as_ref() else {
            return;
        };
        let size = overlay.window.inner_size();
        let theme = overlay.window.theme();
        self.core.handle(Event::Resize {
            width: size.width,
            height: size.height,
        });
        self.core.handle(Event::Theme(theme_of(theme)));
    }

    fn redraw(&mut self) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        let mut buffer = overlay.presenter.buffer();
        if let Some(damage) = self.core.render(&mut buffer, platform::FORMAT) {
            overlay.presenter.present(damage);
        }
    }

    fn open_settings(&mut self) {
        if let Some(window) = &self.settings {
            window.raise();
            return;
        }
        self.settings = SettingsWindow::open(&self.config);
    }

    fn on_settings(&mut self, event: SettingsEvent) {
        match event {
            SettingsEvent::Record(binding, text) => self.rebind(binding, &text),
            SettingsEvent::LaunchAtLogin(on) => {
                let result = platform::set_launch_at_login(on);
                if result.is_ok() {
                    self.config.launch_at_login = on;
                    settings::save(&self.config);
                }
                if let Some(window) = &self.settings {
                    if let Err(error) = result {
                        window.set_launch_at_login(!on);
                        window.set_message(&format!("Could not change launch at login: {error}"));
                    } else {
                        window.set_message("");
                    }
                }
            }
            // Dropping the handle is what returns the window's memory.
            SettingsEvent::Closed => {
                self.settings = None;
                if self.reopen > 0 {
                    self.reopen -= 1;
                    self.open_settings();
                }
            }
        }
    }

    fn rebind(&mut self, binding: Binding, text: &str) {
        let result = self.hotkeys.rebind(binding, text);
        let current = self.hotkeys.text(binding);
        if result.is_ok() {
            match binding {
                Binding::Toggle => self.config.toggle.clone_from(&current),
                Binding::Clear => self.config.clear.clone_from(&current),
            }
            settings::save(&self.config);
        }
        let Some(window) = &self.settings else { return };
        window.set_hotkey(binding, &current);
        let shown = platform::label(text);
        window.set_message(&match result {
            Ok(()) => String::new(),
            Err(RebindError::Taken) => format!(
                "{shown} is already used by another app. Kept {}.",
                platform::label(&current)
            ),
            Err(RebindError::UsedByOther(other)) => format!(
                "{shown} is already the {} hotkey.",
                match other {
                    Binding::Toggle => "Toggle",
                    Binding::Clear => "Clear",
                }
            ),
            Err(RebindError::Invalid) => "That combination can't be used.".to_owned(),
        });
    }

    /// The Clear hotkey: the core drops the Ink, and `sync` destroys the
    /// Overlay because it reports it is no longer needed.
    fn on_clear(&mut self, event_loop: &ActiveEventLoop) {
        let view = self.core.handle(Event::Clear);
        self.sync(event_loop, view, None);
    }

    fn key(&mut self, event_loop: &ActiveEventLoop, event: &KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let key = match &event.logical_key {
            WinitKey::Named(NamedKey::Escape) => Key::Escape,
            WinitKey::Named(NamedKey::Delete | NamedKey::Backspace) => Key::Delete,
            WinitKey::Character(text) => match text.to_lowercase().chars().next() {
                Some(c) => Key::Char(c),
                None => return,
            },
            _ => return,
        };
        self.input(
            event_loop,
            Event::Key {
                key,
                command: platform::command_held(self.modifiers),
                shift: self.modifiers.shift_key(),
            },
        );
        // Cmd/Ctrl+C: the core produced the Excalidraw clipboard JSON.
        if let Some(text) = self.core.take_copy() {
            platform::set_clipboard_text(&text);
        }
    }

    /// Feeds an input event to the core and applies what changed: a toolbar
    /// Clear leaves Draw Mode (`sync`), anything else only repaints and
    /// updates the cursor shape.
    fn input(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        let was_drawing = self.core.view().draw_mode;
        let view = self.core.handle(event);
        if view.draw_mode != was_drawing || !view.overlay_needed {
            self.sync(event_loop, view, None);
            return;
        }
        if view.cursor != self.cursor_shape {
            self.cursor_shape = view.cursor;
            if let Some(overlay) = self.overlay.as_ref() {
                overlay.window.set_cursor(cursor_icon(view.cursor));
            }
        }
        if view.needs_render {
            self.redraw();
        }
    }
}

fn theme_of(theme: Option<OsTheme>) -> Theme {
    match theme {
        Some(OsTheme::Dark) => Theme::Dark,
        _ => Theme::Light,
    }
}

fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Arrow => CursorIcon::Default,
        Cursor::Crosshair => CursorIcon::Crosshair,
    }
}

#[allow(clippy::cast_possible_truncation, reason = "scale factors are small")]
fn scale_of(monitor: &MonitorHandle) -> f32 {
    monitor.scale_factor() as f32
}

fn display_id(monitor: &MonitorHandle) -> DisplayId {
    let position = monitor.position();
    DisplayId((u64::from(position.x.cast_unsigned()) << 32) | u64::from(position.y.cast_unsigned()))
}

fn create_overlay(event_loop: &ActiveEventLoop, monitor: &MonitorHandle) -> Overlay {
    let attributes = platform::window_attributes(
        Window::default_attributes()
            .with_title("Markuli")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_position(monitor.position())
            .with_inner_size(monitor.size()),
    );
    let window = event_loop
        .create_window(attributes)
        .expect("failed to create the Overlay window");
    let presenter = Presenter::new(&window);
    Overlay {
        display: display_id(monitor),
        window,
        presenter,
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // The tray must be created once the loop runs (macOS requirement).
        if self.tray.is_some() {
            return;
        }
        let menu = Menu::new();
        let settings = MenuItem::new("Settings\u{2026}", true, None);
        let quit = MenuItem::new("Quit", true, None);
        self.settings_id = Some(settings.id().clone());
        self.quit_id = Some(quit.id().clone());
        menu.append_items(&[&settings, &quit])
            .expect("failed to build the tray menu");
        // Test hook: the tray menu cannot be clicked from a script. The value
        // is how many times Settings opens (again after each close).
        if let Some(times) = std::env::var("MARKULI_OPEN_SETTINGS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
        {
            self.reopen = times.saturating_sub(1);
            self.open_settings();
        }
        self.tray = Some(
            platform::tray_icon(TrayIconBuilder::new(), tray_icon_image())
                .with_menu(Box::new(menu))
                .with_tooltip("Markuli")
                .build()
                .expect("failed to create the tray icon"),
        );
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Hotkey(e) if e.state() == HotKeyState::Pressed => {
                match self.hotkeys.binding_for(e.id()) {
                    Some(Binding::Toggle) => self.on_toggle(event_loop),
                    Some(Binding::Clear) => self.on_clear(event_loop),
                    None => {}
                }
            }
            UserEvent::Menu(e) if Some(&e.id) == self.settings_id.as_ref() => self.open_settings(),
            UserEvent::Settings(e) => self.on_settings(e),
            UserEvent::Menu(e) if Some(&e.id) == self.quit_id.as_ref() => event_loop.exit(),
            _ => {}
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                self.core.handle(Event::Modifiers {
                    shift: self.modifiers.shift_key(),
                });
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(event_loop, &event),
            WindowEvent::ThemeChanged(theme) => {
                self.input(event_loop, Event::Theme(theme_of(Some(theme))));
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = point(position);
                self.core.handle(Event::Pressure(platform::pen_pressure()));
                self.input(event_loop, Event::PointerMove(self.cursor));
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.core.handle(Event::Pressure(platform::pen_pressure()));
                self.input(
                    event_loop,
                    match state {
                        ElementState::Pressed => Event::PointerDown(self.cursor),
                        ElementState::Released => Event::PointerUp(self.cursor),
                    },
                );
            }
            // Windows pens (and touch) arrive here, with the pen's force.
            WindowEvent::Touch(touch) => {
                self.cursor = point(touch.location);
                let pressure = touch.force.map(|f| {
                    #[allow(clippy::cast_possible_truncation, reason = "0..=1")]
                    let p = f.normalized() as f32;
                    p
                });
                self.core.handle(Event::Pressure(pressure));
                self.input(
                    event_loop,
                    match touch.phase {
                        TouchPhase::Started => Event::PointerDown(self.cursor),
                        TouchPhase::Moved => Event::PointerMove(self.cursor),
                        TouchPhase::Ended | TouchPhase::Cancelled => Event::PointerUp(self.cursor),
                    },
                );
            }
            _ => {}
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "screen coordinates fit f32"
)]
fn point(position: PhysicalPosition<f64>) -> Point {
    Point {
        x: position.x as f32,
        y: position.y as f32,
    }
}

/// A 32x32 monochrome ring; macOS tints template icons for light/dark bars.
fn tray_icon_image() -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (
                f32::from(u8::try_from(x).unwrap_or(0)) - 15.5,
                f32::from(u8::try_from(y).unwrap_or(0)) - 15.5,
            );
            let distance = dx.hypot(dy);
            let coverage = (1.5 - (distance - 11.0).abs()).clamp(0.0, 1.0);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "0..=255"
            )]
            rgba.extend_from_slice(&[0, 0, 0, (coverage * 255.0) as u8]);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("tray icon pixels have a valid size")
}

fn main() {
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    platform::configure_event_loop(&mut builder);
    let event_loop = builder.build().expect("failed to create the event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    let config = settings::load();
    let manager = GlobalHotKeyManager::new().expect("failed to start the hotkey manager");
    let hotkeys = Hotkeys::new(manager, &config);
    let hotkey_proxy = proxy.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e| {
        let _ = hotkey_proxy.send_event(UserEvent::Hotkey(e));
    }));
    let settings_proxy = proxy.clone();
    settings::set_sink(move |e| {
        let _ = settings_proxy.send_event(UserEvent::Settings(e));
    });
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));

    let mut app = App {
        core: Annotator::new(),
        overlay: None,
        cursor: Point { x: 0.0, y: 0.0 },
        cursor_shape: Cursor::Crosshair,
        hotkeys,
        config,
        settings: None,
        settings_id: None,
        reopen: 0,
        modifiers: ModifiersState::empty(),
        quit_id: None,
        tray: None,
    };
    event_loop.run_app(&mut app).expect("the event loop failed");
}
