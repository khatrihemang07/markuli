//! macOS Settings window: plain `AppKit` controls, created on demand and
//! released on close. One `NSWindow` subclass is also the target of its own
//! buttons and the recorder (it receives `keyDown:`), so there is no other
//! Objective-C class.

use crate::hotkeys::{self, Binding, Mods};
use crate::settings::{emit, SettingsEvent};
use markuli_core::{Config, ToolbarPosition};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSEventModifierFlags, NSTextField, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::cell::{Cell, OnceCell, RefCell};

const ESCAPE: u16 = 53;

/// The Toolbar radio buttons, in order; a button's tag is its index.
const POSITIONS: [(&str, ToolbarPosition); 4] = [
    ("Top", ToolbarPosition::Top),
    ("Bottom", ToolbarPosition::Bottom),
    ("Left", ToolbarPosition::Left),
    ("Right", ToolbarPosition::Right),
];

#[derive(Default)]
struct Ivars {
    /// Which hotkey is waiting for a key press, if any.
    recording: Cell<Option<Binding>>,
    /// Combo text per hotkey (Toggle, Clear), restored if recording is cancelled.
    shown: RefCell<[String; 2]>,
    toggle: OnceCell<Retained<NSButton>>,
    clear: OnceCell<Retained<NSButton>>,
    login: OnceCell<Retained<NSButton>>,
    message: OnceCell<Retained<NSTextField>>,
}

define_class!(
    // SAFETY: NSWindow has no subclassing requirements beyond calling super
    // for the methods we override, and `Window` does not implement Drop.
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "MarkuliSettingsWindow"]
    #[ivars = Ivars]
    struct Window;

    impl Window {
        #[unsafe(method(recordToggle:))]
        fn record_toggle(&self, _sender: Option<&AnyObject>) {
            self.start_recording(Binding::Toggle);
        }

        #[unsafe(method(recordClear:))]
        fn record_clear(&self, _sender: Option<&AnyObject>) {
            self.start_recording(Binding::Clear);
        }

        #[unsafe(method(loginChanged:))]
        fn login_changed(&self, _sender: Option<&AnyObject>) {
            let on = self
                .ivars()
                .login
                .get()
                .is_some_and(|b| b.state() == NSControlStateValueOn);
            emit(SettingsEvent::LaunchAtLogin(on));
        }

        #[unsafe(method(positionChanged:))]
        fn position_changed(&self, sender: Option<&AnyObject>) {
            // SAFETY: the sender is one of our radio buttons, an NSButton.
            let tag: isize = sender.map_or(-1, |s| unsafe { msg_send![s, tag] });
            let position = usize::try_from(tag)
                .ok()
                .and_then(|i| POSITIONS.get(i).map(|(_, p)| *p));
            if let Some(position) = position {
                emit(SettingsEvent::Toolbar(position));
            }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let Some(binding) = self.ivars().recording.get() else {
                // SAFETY: forwarding the same event to the superclass.
                unsafe { msg_send![super(self), keyDown: event] }
                return;
            };
            let code = event.keyCode();
            if code == ESCAPE {
                self.stop_recording(binding);
                self.set_message("");
                emit(SettingsEvent::Listening(false));
                return;
            }
            let flags = event.modifierFlags();
            let mods = Mods {
                ctrl: flags.contains(NSEventModifierFlags::Control),
                alt: flags.contains(NSEventModifierFlags::Option),
                shift: flags.contains(NSEventModifierFlags::Shift),
                logo: flags.contains(NSEventModifierFlags::Command),
            };
            match hotkeys::key_from_mac(code).and_then(|key| hotkeys::recorded(mods, key)) {
                Some(text) => {
                    self.stop_recording(binding);
                    emit(SettingsEvent::Record(binding, text));
                    emit(SettingsEvent::Listening(false));
                }
                None => self.set_message("Hold Ctrl, Alt, Shift or Cmd with a key (Esc cancels)."),
            }
        }

        // Closing is the only way out, so the app can drop its handle.
        #[unsafe(method(close))]
        fn close(&self) {
            // SAFETY: plain super call.
            unsafe { msg_send![super(self), close] }
            emit(SettingsEvent::Closed);
        }
    }
);

impl Window {
    fn start_recording(&self, binding: Binding) {
        if let Some(previous) = self.ivars().recording.replace(Some(binding)) {
            self.set_binding_title(previous, None);
        }
        // A combo another app owns never reaches this window (the OS hands it
        // to that app), so say what silence means.
        self.set_message("No reaction? Another app owns it. Esc cancels.");
        if let Some(button) = self.button(binding) {
            button.setTitle(&NSString::from_str("Press the new shortcut..."));
        }
        // Buttons would swallow Space and Return; the window must get the keys.
        self.makeFirstResponder(None);
        emit(SettingsEvent::Listening(true));
    }

    fn stop_recording(&self, binding: Binding) {
        self.ivars().recording.set(None);
        self.set_binding_title(binding, None);
    }

    fn button(&self, binding: Binding) -> Option<&Retained<NSButton>> {
        match binding {
            Binding::Toggle => self.ivars().toggle.get(),
            Binding::Clear => self.ivars().clear.get(),
        }
    }

    fn set_binding_title(&self, binding: Binding, text: Option<&str>) {
        let slot = binding as usize;
        if let Some(text) = text {
            text.clone_into(&mut self.ivars().shown.borrow_mut()[slot]);
        }
        if let Some(button) = self.button(binding) {
            let shown = self.ivars().shown.borrow();
            button.setTitle(&NSString::from_str(&super::label(&shown[slot])));
        }
    }

    fn set_message(&self, text: &str) {
        if let Some(label) = self.ivars().message.get() {
            label.setStringValue(&NSString::from_str(text));
        }
    }
}

/// The "Toolbar" row: a label and four radio buttons, `current` one on.
fn add_toolbar_row(
    content: &NSView,
    target: &AnyObject,
    current: ToolbarPosition,
    mtm: MainThreadMarker,
) {
    let label = NSTextField::labelWithString(&NSString::from_str("Toolbar"), mtm);
    label.setFrame(NSRect::new(
        NSPoint::new(20.0, 114.0),
        NSSize::new(160.0, 20.0),
    ));
    content.addSubview(&label);
    // Radio buttons sharing a superview and an action exclude each other.
    let mut x = 190.0;
    for (tag, (title, position)) in (0_isize..).zip(POSITIONS) {
        // SAFETY: the target is the window, which implements `positionChanged:`.
        let radio = unsafe {
            NSButton::radioButtonWithTitle_target_action(
                &NSString::from_str(title),
                Some(target),
                Some(sel!(positionChanged:)),
                mtm,
            )
        };
        radio.setTag(tag);
        radio.setFrame(NSRect::new(NSPoint::new(x, 110.0), NSSize::new(64.0, 24.0)));
        if position == current {
            radio.setState(NSControlStateValueOn);
        }
        content.addSubview(&radio);
        x += 64.0;
    }
}

/// Handle to the open window. Dropping it releases everything.
pub struct SettingsWindow {
    window: Retained<Window>,
}

impl SettingsWindow {
    pub fn open(config: &Config) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(450.0, 240.0));
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
        let window = Window::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: designated initialiser of NSWindow on the main thread.
        let window: Retained<Window> = unsafe {
            msg_send![super(window), initWithContentRect: frame, styleMask: style,
                backing: NSBackingStoreType::Buffered, defer: false]
        };
        // SAFETY: we own the window through `Retained`; AppKit must not also release it.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str("Markuli Settings"));

        let target: &AnyObject = &window;
        let ivars = window.ivars();
        let content = window.contentView()?;

        let add_row = |y: f64, name: &str, action, binding: Binding, text: &str| {
            let label = NSTextField::labelWithString(&NSString::from_str(name), mtm);
            label.setFrame(NSRect::new(
                NSPoint::new(20.0, y + 4.0),
                NSSize::new(160.0, 20.0),
            ));
            content.addSubview(&label);
            // SAFETY: the target is the window, which implements `action`.
            let button = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(&super::label(text)),
                    Some(target),
                    Some(action),
                    mtm,
                )
            };
            button.setFrame(NSRect::new(
                NSPoint::new(190.0, y),
                NSSize::new(190.0, 28.0),
            ));
            content.addSubview(&button);
            let cell = match binding {
                Binding::Toggle => &ivars.toggle,
                Binding::Clear => &ivars.clear,
            };
            let _ = cell.set(button);
            text.clone_into(&mut ivars.shown.borrow_mut()[binding as usize]);
        };
        add_row(
            190.0,
            "Toggle Draw Mode",
            sel!(recordToggle:),
            Binding::Toggle,
            &config.toggle,
        );
        add_row(
            150.0,
            "Clear",
            sel!(recordClear:),
            Binding::Clear,
            &config.clear,
        );

        add_toolbar_row(&content, target, config.toolbar, mtm);

        // SAFETY: the target is the window, which implements `loginChanged:`.
        let login = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Launch at login"),
                Some(target),
                Some(sel!(loginChanged:)),
                mtm,
            )
        };
        login.setFrame(NSRect::new(
            NSPoint::new(20.0, 70.0),
            NSSize::new(410.0, 24.0),
        ));
        login.setState(if config.launch_at_login {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        content.addSubview(&login);
        let _ = ivars.login.set(login);

        let message = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        message.setFrame(NSRect::new(
            NSPoint::new(20.0, 20.0),
            NSSize::new(410.0, 36.0),
        ));
        message.setTextColor(Some(&NSColor::systemRedColor()));
        content.addSubview(&message);
        let _ = ivars.message.set(message);

        window.center();
        let this = Self { window };
        this.raise();
        Some(this)
    }

    /// Accessory apps are never active by themselves; Settings must take focus.
    pub fn raise(&self) {
        if let Some(mtm) = MainThreadMarker::new() {
            #[allow(deprecated, reason = "`activate` needs macOS 14; we support older")]
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        }
        self.window.makeKeyAndOrderFront(None);
    }

    pub fn set_hotkey(&self, binding: Binding, text: &str) {
        self.window.set_binding_title(binding, Some(text));
    }

    pub fn set_message(&self, text: &str) {
        self.window.set_message(text);
    }

    pub fn set_launch_at_login(&self, on: bool) {
        if let Some(login) = self.window.ivars().login.get() {
            login.setState(if on {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
    }
}
