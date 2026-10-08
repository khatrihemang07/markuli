//! Markuli: tray app that draws Ink over the display under the cursor.
//!
//! This file is the event-loop glue: OS events become core events, core
//! `View` state becomes window calls. Logic belongs in `markuli-core`.

mod cursor;
mod editors;
mod hotkeys;
mod platform;
mod settings;

use editors::EditorEvent;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use hotkeys::{Binding, Hotkeys, RebindError};
use markuli_core::{Annotator, Config, Cursor, DisplayId, Event, Insets, Key, Point, Theme, View};
use platform::{Presenter, SettingsWindow};
use settings::SettingsEvent;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, StartCause, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::monitor::MonitorHandle;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Theme as OsTheme, Window, WindowId};

/// Wake-ups sent from the hotkey and menu callbacks, so the loop never polls.
enum UserEvent {
    Hotkey(GlobalHotKeyEvent),
    Menu(MenuEvent),
    Settings(SettingsEvent),
    Editor(EditorEvent),
}

/// The Overlay window and what it draws through.
struct Overlay {
    window: Window,
    presenter: Presenter,
    /// The mouse cursor shown over the canvas (picture built on change only).
    cursor: platform::Shape,
}

struct App {
    core: Annotator,
    /// The origin of the core's clock (`Event::Clock` milliseconds).
    started: Instant,
    overlay: Option<Overlay>,
    /// The app that had the focus before Draw Mode began; gets it back when
    /// Draw Mode ends.
    previous: Option<platform::Previous>,
    cursor: Point,
    cursor_shape: Cursor,
    hotkeys: Hotkeys<GlobalHotKeyManager>,
    config: Config,
    /// Exists only while the Settings window is open.
    settings: Option<SettingsWindow>,
    settings_id: Option<MenuId>,
    /// Test hook (`dev-hooks`): reopen Settings this many more times after it
    /// closes.
    #[cfg(feature = "dev-hooks")]
    reopen: u32,
    modifiers: ModifiersState,
    /// The left press in flight was turned into a secondary click, so its
    /// release is not a pointer event.
    secondary_press: bool,
    quit_id: Option<MenuId>,
    tray: Option<TrayIcon>,
}

impl App {
    /// The current time as a core event; pointer events need it for the Laser.
    fn clock(&self) -> Event {
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Event::Clock(ms)
    }

    fn on_toggle(&mut self, event_loop: &ActiveEventLoop) {
        let Some(monitor) = platform::monitor_under_cursor(event_loop)
            .or_else(|| event_loop.primary_monitor())
            .or_else(|| event_loop.available_monitors().next())
        else {
            return;
        };
        let display = display_id(&monitor);
        let view = self.core.handle(Event::ToggleDrawMode(display));
        self.sync(event_loop, view, Some(&monitor));
    }

    /// Brings the window in line with what the core says: the Overlay exists
    /// exactly while Draw Mode is on (ADR-0002), so it is always created on
    /// the display the toggle happened on.
    fn sync(&mut self, event_loop: &ActiveEventLoop, view: View, monitor: Option<&MonitorHandle>) {
        if !view.draw_mode {
            if self.overlay.take().is_some() {
                platform::close_editor(&mut |event| {
                    self.core.handle(event);
                });
                // The Overlay's key events are gone with it, and so is the
                // chance to see a modifier being released.
                self.forget_modifiers();
                if let Some(previous) = self.previous.take() {
                    previous.restore();
                }
                platform::release_memory();
            }
            return;
        }
        if self.overlay.is_none() {
            let Some(monitor) = monitor else { return };
            self.previous = platform::frontmost_other();
            self.overlay = Some(create_overlay(event_loop, monitor));
            self.core.handle(Event::ScaleFactor(scale_of(monitor)));
            self.send_surface(platform::insets(monitor));
            self.core.handle(Event::SurfaceReset);
        }
        if self.overlay.is_none() {
            return;
        }
        self.show_cursor(event_loop, view.cursor);
        // First frame is painted before the window shows: no flash.
        self.redraw();
        if let Some(overlay) = self.overlay.as_ref() {
            overlay.window.set_visible(true);
            // After showing: winit's `focus_window` does nothing for a hidden
            // window, and the app (an Accessory one) must be activated for the
            // Overlay to be the key window, or no key reaches it.
            overlay.window.focus_window();
        }
    }

    /// Makes `cursor` the one over the canvas. Called only when the core's
    /// choice (Tool, color, width) or the scale changes, never per move: the
    /// picture is drawn here, once.
    fn show_cursor(&mut self, event_loop: &ActiveEventLoop, cursor: Cursor) {
        self.cursor_shape = cursor;
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        let scale = overlay.window.scale_factor();
        #[allow(clippy::cast_possible_truncation, reason = "scale factors are small")]
        let image = cursor::render(cursor, scale as f32);
        overlay.cursor = platform::Shape::new(event_loop, image.as_ref(), scale);
        overlay.cursor.set(&overlay.window);
    }

    /// Tells the core the new Overlay's size and the OS theme. winit reads the
    /// theme from `effectiveAppearance` on macOS and from the
    /// `AppsUseLightTheme` registry value on Windows.
    fn send_surface(&mut self, insets: Insets) {
        let Some(overlay) = self.overlay.as_ref() else {
            return;
        };
        let size = overlay.window.inner_size();
        let theme = overlay.window.theme();
        self.core.handle(Event::Resize {
            width: size.width,
            height: size.height,
        });
        self.core.handle(Event::Insets(insets));
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

    /// Draw Mode needs keys (Tool shortcuts, Esc, undo), and keys go to the
    /// key window of the active app. When another app took focus while Draw
    /// Mode was on, a click on the Overlay brings it back.
    fn reclaim_focus(&self) {
        if let Some(overlay) = self.overlay.as_ref() {
            if self.core.view().draw_mode && !overlay.window.has_focus() {
                overlay.window.focus_window();
            }
        }
    }

    /// No modifier is held as far as Markuli knows (see `Focused(false)`).
    fn forget_modifiers(&mut self) {
        self.modifiers = ModifiersState::empty();
        self.core.handle(Event::Modifiers { shift: false });
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
            SettingsEvent::Listening(true) => self.hotkeys.suspend(),
            SettingsEvent::Listening(false) => self.hotkeys.resume(),
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
            SettingsEvent::Toolbar(position) => {
                self.config.toolbar = position;
                settings::save(&self.config);
                let view = self.core.handle(Event::ToolbarPosition(position));
                if view.needs_render {
                    self.redraw();
                }
            }
            // Dropping the handle is what returns the window's memory.
            SettingsEvent::Closed => {
                self.hotkeys.resume();
                self.settings = None;
                #[cfg(feature = "dev-hooks")]
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

    /// The Clear hotkey: the core drops the Ink and Draw Mode stays as it
    /// is, so there is only a repaint, and none outside Draw Mode.
    fn on_clear(&mut self) {
        let view = self.core.handle(Event::Clear);
        if view.needs_render {
            self.redraw();
        }
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
                alt: self.modifiers.alt_key(),
            },
        );
        // Cmd/Ctrl+C: the core produced the Excalidraw clipboard JSON.
        if let Some(text) = self.core.take_copy() {
            platform::set_clipboard_text(&text);
        }
    }

    /// Saves the config when the Pen's Style or the Palette changed, and
    /// only then.
    fn remember_style(&mut self, view: View) {
        if view.style != self.config.style || view.palette != self.config.palette {
            self.config.style = view.style;
            self.config.palette = view.palette;
            settings::save(&self.config);
        }
    }

    /// A right click, or Control+click on macOS. On a Palette button the
    /// core reports `View::edit` and the platform editor runs until closed.
    fn secondary_click(&mut self, event_loop: &ActiveEventLoop) {
        // An editor still open ends its edit before the new request arrives.
        platform::close_editor(&mut |event| self.input(event_loop, event));
        self.input(event_loop, Event::SecondaryClick(self.cursor));
        let Some(request) = self.core.view().edit else {
            return;
        };
        // The raw handle is Copy, so the editor does not borrow the Overlay
        // while its edits come back through `input`.
        let owner = self
            .overlay
            .as_ref()
            .and_then(|overlay| overlay.window.window_handle().ok())
            .map(|handle| handle.as_raw());
        if let Some(owner) = owner {
            platform::open_editor(request, owner, &mut |event| self.input(event_loop, event));
        }
    }

    /// An editor changed a value or closed: the core hears it like any input,
    /// so the Palette is saved and the Toolbar and Selection repainted.
    fn on_editor(&mut self, event_loop: &ActiveEventLoop, event: EditorEvent) {
        let event = match event {
            EditorEvent::Color { slot, rgb } => Event::EditColor { slot, rgb },
            EditorEvent::Width { slot, width } => Event::EditWidth { slot, width },
            EditorEvent::Closed { generation } => {
                platform::editor_closed(generation, &mut |event| self.input(event_loop, event));
                return;
            }
        };
        self.input(event_loop, event);
    }

    /// Feeds an input event to the core and applies what changed: Draw Mode
    /// ending (`sync`), or a repaint and the cursor shape.
    fn input(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        let was_drawing = self.core.view().draw_mode;
        let view = self.core.handle(event);
        self.remember_style(view);
        if view.draw_mode != was_drawing {
            self.sync(event_loop, view, None);
            return;
        }
        if view.cursor != self.cursor_shape {
            self.show_cursor(event_loop, view.cursor);
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

#[allow(clippy::cast_possible_truncation, reason = "scale factors are small")]
fn scale_of(monitor: &MonitorHandle) -> f32 {
    monitor.scale_factor() as f32
}

/// A display is the same display only if position, size and scale all match:
/// the Overlay must be rebuilt when the cursor's display changes resolution or
/// scale at the same position (ADR-0002), and the Ink Clears with it.
fn display_id(monitor: &MonitorHandle) -> DisplayId {
    use std::hash::{Hash, Hasher};
    let (position, size) = (monitor.position(), monitor.size());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (
        position.x,
        position.y,
        size.width,
        size.height,
        monitor.scale_factor().to_bits(),
    )
        .hash(&mut hasher);
    DisplayId::new(hasher.finish())
}

fn create_overlay(event_loop: &ActiveEventLoop, monitor: &MonitorHandle) -> Overlay {
    // winit's monitor values are physical pixels of that monitor, but a window
    // is placed and sized in the coordinates of the screen it lands on, so a
    // mixed-DPI setup needs them converted with the monitor's own scale.
    let scale = monitor.scale_factor();
    let attributes = platform::window_attributes(
        Window::default_attributes()
            .with_title("Markuli")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_position(monitor.position().to_logical::<f64>(scale))
            .with_inner_size(monitor.size().to_logical::<f64>(scale)),
    );
    let window = event_loop
        .create_window(attributes)
        .expect("failed to create the Overlay window");
    let presenter = Presenter::new(&window);
    let cursor = platform::Shape::new(event_loop, None, scale);
    Overlay {
        window,
        presenter,
        cursor,
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
        // Test hook, absent from release builds: the tray menu cannot be
        // clicked from a script. The value is how many times Settings opens
        // (again after each close).
        #[cfg(feature = "dev-hooks")]
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
                    Some(Binding::Clear) => self.on_clear(),
                    None => {}
                }
            }
            UserEvent::Menu(e) if Some(&e.id) == self.settings_id.as_ref() => self.open_settings(),
            UserEvent::Settings(e) => self.on_settings(e),
            UserEvent::Editor(e) => self.on_editor(event_loop, e),
            UserEvent::Menu(e) if Some(&e.id) == self.quit_id.as_ref() => event_loop.exit(),
            _ => {}
        }
    }

    /// A frame is due: the Laser is fading. Only the core schedules one
    /// (`about_to_wait`), so the loop sleeps once the trail is gone.
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.input(event_loop, self.clock());
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(match self.core.view().next_frame {
            Some(ms) => ControlFlow::WaitUntil(self.started + Duration::from_millis(ms)),
            None => ControlFlow::Wait,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            // A window that is not key never sees a modifier being released,
            // so what it remembers would be stale (Alt left "held" made every
            // Tool key do nothing).
            WindowEvent::Focused(false) => self.forget_modifiers(),
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
                if let Some(overlay) = self.overlay.as_ref() {
                    overlay.cursor.keep(&overlay.window);
                }
                self.core.handle(self.clock());
                self.core.handle(Event::Pressure(platform::pen_pressure()));
                self.input(event_loop, Event::PointerMove(self.cursor));
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => self.secondary_click(event_loop),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                // The focus may be elsewhere (an editor took it), and then
                // winit's modifier state is stale: ask the press itself.
                if state == ElementState::Pressed {
                    self.reclaim_focus();
                }
                if state == ElementState::Pressed && platform::secondary_click_modifier() {
                    self.secondary_press = true;
                    self.secondary_click(event_loop);
                    return;
                }
                if state == ElementState::Released && std::mem::take(&mut self.secondary_press) {
                    return;
                }
                self.core.handle(self.clock());
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
                if touch.phase == TouchPhase::Started {
                    self.reclaim_focus();
                }
                self.core.handle(self.clock());
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
    let editor_proxy = proxy.clone();
    editors::set_sink(move |e| {
        let _ = editor_proxy.send_event(UserEvent::Editor(e));
    });
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));

    let mut core = Annotator::new();
    core.handle(Event::Palette(config.palette));
    core.handle(Event::Style(config.style));
    core.handle(Event::ToolbarPosition(config.toolbar));

    let mut app = App {
        core,
        started: Instant::now(),
        overlay: None,
        previous: None,
        cursor: Point { x: 0.0, y: 0.0 },
        cursor_shape: Cursor::Arrow,
        hotkeys,
        config,
        settings: None,
        settings_id: None,
        #[cfg(feature = "dev-hooks")]
        reopen: 0,
        modifiers: ModifiersState::empty(),
        secondary_press: false,
        quit_id: None,
        tray: None,
    };
    event_loop.run_app(&mut app).expect("the event loop failed");
}
