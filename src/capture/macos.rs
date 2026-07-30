//! The macOS half of quick capture.
//!
//! Two system calls do the work, both behind Accessibility:
//!
//! * a **CoreGraphics event tap**, listen-only, to hear the watched modifier
//!   go up and down. A bare Shift can't be a registered shortcut, so there is
//!   no lighter way to hear one.
//! * the **Accessibility API**, to ask the focused element what text is
//!   selected. Deliberately not a synthetic Cmd+C: that would silently
//!   overwrite whatever you had on the clipboard, which is exactly the kind of
//!   thing a capture tool must never do.
//!
//! Nothing here decides anything. The tap reports transitions to
//! [`super::Detector`] and the reader hands raw text to [`super::split`], so
//! the policy stays testable on any platform and this file stays a binding.

use super::{split, Captured, Config, Detector, Signal, TapKey};
use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ types

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFIndex = isize;
type CFTypeID = usize;
/// CoreFoundation's `Boolean` is an `unsigned char`, not a C99 `bool`.
/// Declaring it as a Rust `bool` would be undefined the moment a function
/// returned anything other than 0 or 1.
type Boolean = u8;

type AXUIElementRef = *const c_void;
type AXError = i32;

type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;
type CGEventType = u32;
type CGEventMask = u64;
type CGEventTapCallBack = extern "C" fn(
    proxy: CGEventTapProxy,
    kind: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

/// An extern static whose layout we never need — only its address.
#[repr(C)]
struct Opaque {
    _private: [u8; 0],
}

// --------------------------------------------------------------- constants

const UTF8: u32 = 0x0800_0100;

const AX_SUCCESS: AXError = 0;
/// Long enough for a slow app to answer, short enough that a wedged one can't
/// hold the reader thread while you wait for a window that never arrives.
const AX_TIMEOUT_SECS: f32 = 0.25;

const TAP_SESSION: u32 = 1; // kCGSessionEventTap
const TAP_HEAD_INSERT: u32 = 0; // kCGHeadInsertEventTap
const TAP_LISTEN_ONLY: u32 = 1; // kCGEventTapOptionListenOnly

const EVENT_LEFT_MOUSE_DOWN: CGEventType = 1;
const EVENT_RIGHT_MOUSE_DOWN: CGEventType = 3;
const EVENT_KEY_DOWN: CGEventType = 10;
const EVENT_FLAGS_CHANGED: CGEventType = 12;
const EVENT_OTHER_MOUSE_DOWN: CGEventType = 25;
const EVENT_TAP_DISABLED_BY_TIMEOUT: CGEventType = 0xFFFF_FFFE;
const EVENT_TAP_DISABLED_BY_USER_INPUT: CGEventType = 0xFFFF_FFFF;

const FLAG_CAPS_LOCK: u64 = 0x0001_0000;
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALTERNATE: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_SECONDARY_FN: u64 = 0x0080_0000;

/// The bits worth comparing. `CGEventFlags` also carries housekeeping bits
/// such as `NonCoalesced`, which flip on their own and would otherwise read as
/// "some other modifier moved" on every single event.
const FLAG_MODIFIERS: u64 =
    FLAG_CAPS_LOCK | FLAG_SHIFT | FLAG_CONTROL | FLAG_ALTERNATE | FLAG_COMMAND | FLAG_SECONDARY_FN;

fn mask_of(key: TapKey) -> u64 {
    match key {
        TapKey::Shift => FLAG_SHIFT,
        TapKey::Control => FLAG_CONTROL,
        TapKey::Alt => FLAG_ALTERNATE,
        TapKey::Command => FLAG_COMMAND,
    }
}

// --------------------------------------------------------------------- ffi

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFStringRef;
    static kCFRunLoopCommonModes: CFStringRef;
    static kCFTypeDictionaryKeyCallBacks: Opaque;
    static kCFTypeDictionaryValueCallBacks: Opaque;
    static kCFBooleanTrue: CFTypeRef;

    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;

    fn CFStringGetTypeID() -> CFTypeID;
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cstr: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    fn CFStringGetMaximumSizeForEncoding(len: CFIndex, encoding: u32) -> CFIndex;
    fn CFStringGetCString(
        s: CFStringRef,
        buffer: *mut c_char,
        size: CFIndex,
        encoding: u32,
    ) -> Boolean;

    fn CFDictionaryCreate(
        alloc: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;

    fn CFMachPortCreateRunLoopSource(
        alloc: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    fn CFMachPortInvalidate(port: CFMachPortRef);

    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source: Boolean) -> i32;
    fn CFRunLoopStop(rl: CFRunLoopRef);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;

    fn AXIsProcessTrusted() -> Boolean;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> AXError;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    /// Takes a C99 `bool`, not a CoreFoundation `Boolean` — CoreGraphics and
    /// CoreFoundation disagree about what a boolean is, and they are both here.
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetFlags(event: CGEventRef) -> u64;
}

// ------------------------------------------------------------- permissions

/// True once Flodo appears in Privacy & Security → Accessibility.
///
/// `prompt` shows the system's own "open System Settings" sheet. It only
/// appears once per app per install, so it is spent on the moment the user
/// asks for quick capture and never on startup.
fn trusted(prompt: bool) -> bool {
    if !prompt {
        return unsafe { AXIsProcessTrusted() != 0 };
    }
    unsafe {
        let key = kAXTrustedCheckOptionPrompt;
        let value = kCFBooleanTrue;
        let options = CFDictionaryCreate(
            ptr::null(),
            &key as *const CFStringRef as *const *const c_void,
            &value as *const CFTypeRef as *const *const c_void,
            1,
            &raw const kCFTypeDictionaryKeyCallBacks as *const c_void,
            &raw const kCFTypeDictionaryValueCallBacks as *const c_void,
        );
        if options.is_null() {
            return AXIsProcessTrusted() != 0;
        }
        let ok = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        ok != 0
    }
}

// ------------------------------------------------------------ reading text

/// Builds a CFString from a static name. Returns null on failure, which every
/// caller already treats as "no answer".
unsafe fn cf_string(name: &CStr) -> CFStringRef {
    CFStringCreateWithCString(ptr::null(), name.as_ptr(), UTF8)
}

/// Copies one Accessibility attribute. The result is owned — release it.
unsafe fn copy_attribute(element: AXUIElementRef, name: &CStr) -> Option<CFTypeRef> {
    if element.is_null() {
        return None;
    }
    let attribute = cf_string(name);
    if attribute.is_null() {
        return None;
    }
    let mut value: CFTypeRef = ptr::null();
    let err = AXUIElementCopyAttributeValue(element, attribute, &mut value);
    CFRelease(attribute);
    if err == AX_SUCCESS && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

/// Converts a CFString to a Rust `String`. Borrows it — does not release it.
///
/// The type check is not a formality: `AXSelectedText` is documented as a
/// string, but the value comes from another process's accessibility
/// implementation, and treating whatever arrives as a `CFStringRef` on trust
/// is how a misbehaving app would take Flodo down with it.
unsafe fn read_string(value: CFTypeRef) -> Option<String> {
    if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
        return None;
    }
    let len = CFStringGetLength(value);
    if len <= 0 {
        return None;
    }
    let capacity = CFStringGetMaximumSizeForEncoding(len, UTF8) + 1;
    if capacity <= 0 {
        return None;
    }
    let mut buffer = vec![0u8; capacity as usize];
    if CFStringGetCString(value, buffer.as_mut_ptr() as *mut c_char, capacity, UTF8) == 0 {
        return None;
    }
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    buffer.truncate(end);
    String::from_utf8(buffer).ok()
}

/// Asks the frontmost app what text is selected right now.
///
/// Returns `None` whenever there is no answer — nothing selected, an app that
/// doesn't implement the attribute (some Electron and Java apps, and browsers
/// until their own accessibility is switched on), or a timeout. All of these
/// are ordinary: the double-tap then summons Flodo with an empty composer,
/// which is still exactly what was asked for.
fn selected_text() -> Option<String> {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        AXUIElementSetMessagingTimeout(system, AX_TIMEOUT_SECS);

        let focused = copy_attribute(system, c"AXFocusedUIElement");
        CFRelease(system);
        let focused = focused?;

        let selected = copy_attribute(focused as AXUIElementRef, c"AXSelectedText");
        CFRelease(focused);
        let selected = selected?;

        let text = read_string(selected);
        CFRelease(selected);
        text
    }
}

// -------------------------------------------------------------- the listener

/// Lives for as long as the run loop, reached only from the tap callback.
struct TapState {
    detector: Detector,
    mask: u64,
    previous: u64,
    tap: CFMachPortRef,
    triggers: Sender<()>,
}

/// Must never panic or unwind: it is called from C.
extern "C" fn on_event(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    if user_info.is_null() {
        return event;
    }
    // Safe by construction: the box outlives the run loop that calls this, and
    // a listen-only tap is only ever invoked from its own run loop thread.
    let state = unsafe { &mut *(user_info as *mut TapState) };

    // The system switches a tap off if it ever misbehaves. A listen-only tap
    // shouldn't time out, but if it is ever switched off, silence would look
    // exactly like a feature that just stopped working.
    if kind == EVENT_TAP_DISABLED_BY_TIMEOUT || kind == EVENT_TAP_DISABLED_BY_USER_INPUT {
        unsafe { CGEventTapEnable(state.tap, true) };
        return event;
    }

    let signal = match kind {
        EVENT_FLAGS_CHANGED => {
            let flags = unsafe { CGEventGetFlags(event) } & FLAG_MODIFIERS;
            let was_down = state.previous & state.mask != 0;
            let is_down = flags & state.mask != 0;
            state.previous = flags;
            match (was_down, is_down) {
                (false, true) => Signal::Down,
                (true, false) => Signal::Up,
                // Some other modifier moved, which interrupts a tap.
                _ => Signal::Other,
            }
        }
        EVENT_KEY_DOWN
        | EVENT_LEFT_MOUSE_DOWN
        | EVENT_RIGHT_MOUSE_DOWN
        | EVENT_OTHER_MOUSE_DOWN => Signal::Other,
        _ => return event,
    };

    if state.detector.feed(signal, Instant::now()) {
        // Reading the selection is a blocking call into another process, so it
        // happens on the reader thread. Doing it here would stall the tap and
        // stutter every keystroke on the machine.
        let _ = state.triggers.send(());
    }
    event
}

pub struct Backend {
    /// `CFRunLoopRef` as an integer, because a raw pointer isn't `Send` and
    /// this is read from whichever thread drops the watcher.
    runloop: usize,
    stopping: Arc<AtomicBool>,
}

pub fn start(config: Config) -> Result<(Receiver<Captured>, Backend), String> {
    if !trusted(true) {
        return Err(
            "Quick capture needs Accessibility permission. Turn Flodo on in System Settings \
             → Privacy & Security → Accessibility, then switch it on again."
                .to_string(),
        );
    }

    let (trigger_tx, trigger_rx) = channel::<()>();
    let (capture_tx, capture_rx) = channel::<Captured>();
    // The tap thread reports whether it got off the ground, and hands back the
    // run loop that stopping it needs.
    let (ready_tx, ready_rx) = channel::<Result<usize, String>>();

    let stopping = Arc::new(AtomicBool::new(false));

    let tap_stopping = Arc::clone(&stopping);
    std::thread::Builder::new()
        .name("flodo-capture-tap".into())
        .spawn(move || run_tap(config, trigger_tx, ready_tx, tap_stopping))
        .map_err(|e| format!("Could not start the quick capture listener: {e}"))?;

    std::thread::Builder::new()
        .name("flodo-capture-read".into())
        .spawn(move || {
            // Ends when the tap thread drops its sender.
            while trigger_rx.recv().is_ok() {
                let captured = selected_text()
                    .as_deref()
                    .and_then(split)
                    .unwrap_or_default();
                if capture_tx.send(captured).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| format!("Could not start the quick capture reader: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(runloop)) => Ok((capture_rx, Backend { runloop, stopping })),
        Ok(Err(message)) => Err(message),
        Err(_) => Err("The quick capture listener did not start.".to_string()),
    }
}

fn run_tap(
    config: Config,
    triggers: Sender<()>,
    ready: Sender<Result<usize, String>>,
    stopping: Arc<AtomicBool>,
) {
    let events: CGEventMask = (1 << EVENT_KEY_DOWN)
        | (1 << EVENT_FLAGS_CHANGED)
        | (1 << EVENT_LEFT_MOUSE_DOWN)
        | (1 << EVENT_RIGHT_MOUSE_DOWN)
        | (1 << EVENT_OTHER_MOUSE_DOWN);

    let state = Box::into_raw(Box::new(TapState {
        detector: Detector::new(config.window),
        mask: mask_of(config.key),
        previous: 0,
        tap: ptr::null_mut(),
        triggers,
    }));

    unsafe {
        let tap = CGEventTapCreate(
            TAP_SESSION,
            TAP_HEAD_INSERT,
            TAP_LISTEN_ONLY,
            events,
            on_event,
            state as *mut c_void,
        );
        if tap.is_null() {
            drop(Box::from_raw(state));
            // The Accessibility check already passed, so this is the other
            // gate on an event tap: Input Monitoring, which is a separate
            // switch. It is also what you see when permission was granted to
            // an earlier build and the binary has since been replaced —
            // macOS remembers the signature, not the path.
            let _ = ready.send(Err("macOS refused the quick capture listener. \
                 Check System Settings → Privacy & Security → Input Monitoring. \
                 If Flodo is already listed there and in Accessibility, remove it \
                 from both and add it again."
                .to_string()));
            return;
        }
        // The callback re-enables the tap if the system ever switches it off,
        // so it needs the handle it was created with.
        (*state).tap = tap;

        let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
        if source.is_null() {
            CFMachPortInvalidate(tap);
            CFRelease(tap as CFTypeRef);
            drop(Box::from_raw(state));
            let _ = ready.send(Err(
                "Could not attach the quick capture listener.".to_string()
            ));
            return;
        }

        let runloop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(runloop, source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);

        if ready.send(Ok(runloop as usize)).is_err() {
            // Nobody is waiting any more, so there is nothing to listen for.
            stopping.store(true, Ordering::SeqCst);
        }

        // Not a plain `CFRunLoopRun`. `CFRunLoopStop` on a run loop that isn't
        // running yet does nothing, so a watcher dropped in the moment between
        // starting and running would leave this thread blocked for the life of
        // the process. Waking to re-read the flag closes that window; four
        // wake-ups a second on an otherwise idle thread costs nothing.
        while !stopping.load(Ordering::SeqCst) {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.25, 0);
        }

        CFRunLoopRemoveSource(runloop, source, kCFRunLoopCommonModes);
        CFRelease(source as CFTypeRef);
        CGEventTapEnable(tap, false);
        CFMachPortInvalidate(tap);
        CFRelease(tap as CFTypeRef);
        // Last, so the callback can't still be reached through it.
        drop(Box::from_raw(state));
    }
}

pub fn stop(backend: &Backend) {
    backend.stopping.store(true, Ordering::SeqCst);
    // `CFRunLoopStop` is documented as safe to call from another thread, which
    // is the whole reason the run loop is kept rather than a thread handle.
    unsafe { CFRunLoopStop(backend.runloop as CFRunLoopRef) };
}
