//! Event handling — matches JankyBorders events.c + main.c event loop exactly.
//!
//! JB flow:
//! 1. SLSGetEventPort(cid) → mach port
//! 2. CFMachPort wrapping that port, callback drains SLEventCreateNextEvent
//! 3. SLSRegisterNotifyProc for each event type → notify procs fire inside drain
//! 4. CFRunLoopRun() blocks forever

use crate::ffi::skylight::*;
use core_foundation::base::CFTypeRef;
use core_foundation::runloop::*;
use core_foundation::string::CFStringRef;
use std::ffi::c_void;
use tracing::{debug, trace};

// ── Event enum ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum WindowEvent {
    Created(WindowID, SpaceID),
    Destroyed(WindowID, SpaceID),
    Moved(WindowID),
    Resized(WindowID),
    Reordered(WindowID),
    LevelChanged(WindowID),
    Hidden(WindowID),
    Unhidden(WindowID),
    TitleChanged(#[allow(dead_code)] WindowID),
    WindowUpdate(#[allow(dead_code)] WindowID),
    WindowClose(WindowID),
    SpaceChanged,
    FrontChanged,
    PeriodicCleanup,
}

// ── Global handler ───────────────────────────────────────────────────────────

static mut EVENT_HANDLER: Option<Box<dyn FnMut(WindowEvent)>> = None;
static mut GLOBAL_CID: ConnectionID = 0;

fn dispatch(event: WindowEvent) {
    unsafe {
        if let Some(ref mut h) = EVENT_HANDLER {
            trace!(?event, "dispatching");
            h(event);
        }
    }
}

// ── Notify proc callbacks ────────────────────────────────────────────────────

extern "C" fn window_spawn_callback(event: u32, data: *mut c_void, _len: usize, _ctx: *mut c_void) {
    let sid = unsafe { std::ptr::read_unaligned(data as *const u64) };
    let wid = unsafe { std::ptr::read_unaligned((data as *const u8).add(8) as *const u32) };
    match event {
        1325 => dispatch(WindowEvent::Created(wid, sid)),
        1326 => dispatch(WindowEvent::Destroyed(wid, sid)),
        _ => {}
    }
}

extern "C" fn window_modify_callback(event: u32, data: *mut c_void, _len: usize, _ctx: *mut c_void) {
    let wid = unsafe { *(data as *const u32) };
    match event {
        804  => dispatch(WindowEvent::WindowClose(wid)),
        806  => dispatch(WindowEvent::Moved(wid)),
        807  => dispatch(WindowEvent::Resized(wid)),
        808  => dispatch(WindowEvent::Reordered(wid)),
        811  => dispatch(WindowEvent::LevelChanged(wid)),
        815  => dispatch(WindowEvent::Unhidden(wid)),
        816  => dispatch(WindowEvent::Hidden(wid)),
        1322 => dispatch(WindowEvent::TitleChanged(wid)),
        723  => dispatch(WindowEvent::WindowUpdate(wid)),
        _    => {}
    }
}

extern "C" fn space_callback(_: u32, _: *mut c_void, _: usize, _: *mut c_void) {
    dispatch(WindowEvent::SpaceChanged);
}

extern "C" fn front_callback(_: u32, _: *mut c_void, _: usize, _: *mut c_void) {
    dispatch(WindowEvent::FrontChanged);
}

// ── Event loop (matches JB main.c exactly) ───────────────────────────────────

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateWithPort(
        alloc: CFTypeRef, port: u32,
        callback: extern "C" fn(CFTypeRef, *mut c_void, isize, *mut c_void),
        context: *const c_void, should_free: *mut bool,
    ) -> CFTypeRef;
    fn CFMachPortCreateRunLoopSource(alloc: CFTypeRef, port: CFTypeRef, order: isize) -> CFRunLoopSourceRef;
    fn _CFMachPortSetOptions(port: CFTypeRef, options: i32);
    fn CFRelease(cf: CFTypeRef);
    fn CFRunLoopRun();
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: CFTypeRef, mode: CFStringRef);
    fn CFRunLoopTimerCreate(
        alloc: CFTypeRef,
        fire_date: f64,
        interval: f64,
        flags: u64,
        order: i64,
        callback: extern "C" fn(CFTypeRef),
        context: *const c_void,
    ) -> CFTypeRef;
    fn CFAbsoluteTimeGetCurrent() -> f64;
}

extern "C" fn cleanup_timer_callback(_timer: CFTypeRef) {
    dispatch(WindowEvent::PeriodicCleanup);
}

extern "C" fn event_port_callback(_port: CFTypeRef, _msg: *mut c_void, _size: isize, _ctx: *mut c_void) {
    unsafe {
        let mut event = SLEventCreateNextEvent(GLOBAL_CID);
        if event.is_null() { return; }
        loop {
            CFRelease(event);
            event = SLEventCreateNextEvent(GLOBAL_CID);
            if event.is_null() { break; }
        }
    }
}

/// Register all notify procs and enter CFRunLoop. Blocks forever.
pub unsafe fn run_event_loop(cid: ConnectionID, handler: Box<dyn FnMut(WindowEvent)>) {
    GLOBAL_CID = cid;
    EVENT_HANDLER = Some(handler);

    let ctx = cid as *mut c_void;

    // Register all notify procs — matching JB events_register() exactly
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowClose as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowMove as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowResize as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowLevel as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowUnhide as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowHide as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowTitle as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowReorder as u32, ctx);
    SLSRegisterNotifyProc(window_modify_callback, EventType::WindowUpdate as u32, ctx);
    SLSRegisterNotifyProc(window_spawn_callback, EventType::WindowCreate as u32, ctx);
    SLSRegisterNotifyProc(window_spawn_callback, EventType::WindowDestroy as u32, ctx);
    SLSRegisterNotifyProc(space_callback, EventType::SpaceChange as u32, ctx);
    SLSRegisterNotifyProc(front_callback, EventType::FrontChange as u32, ctx);

    debug!("Registered all notify procs");

    // Set up SLSGetEventPort → CFMachPort → CFRunLoop (matches JB main.c)
    let mut port: u32 = 0;
    let err = SLSGetEventPort(cid, &mut port);
    if err == 0 {
        let cf_mach_port = CFMachPortCreateWithPort(
            std::ptr::null(), port, event_port_callback,
            std::ptr::null(), std::ptr::null_mut(),
        );
        _CFMachPortSetOptions(cf_mach_port, 0x40);
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), cf_mach_port, 0);
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);
        CFRelease(cf_mach_port);
        CFRelease(source as CFTypeRef);
    }

    // Periodic cleanup timer — every 60s, dispatch a cleanup event
    let timer = CFRunLoopTimerCreate(
        std::ptr::null(),
        CFAbsoluteTimeGetCurrent() + 60.0,
        60.0, // interval
        0, 0,
        cleanup_timer_callback,
        std::ptr::null(),
    );
    CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopDefaultMode);
    CFRelease(timer);

    debug!("Entering CFRunLoop");
    CFRunLoopRun();
}
