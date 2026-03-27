//! FFI bindings for macOS SkyLight Private Framework

use core_foundation::base::CFTypeRef;
use core_foundation::array::CFArrayRef;
use core_foundation::string::CFStringRef;
use core_graphics::geometry::{CGAffineTransform, CGRect, CGPoint};
use core_graphics::base::CGError;
use std::sync::OnceLock;

/// Connection ID to the window server
pub type ConnectionID = i32;
/// Window ID
pub type WindowID = u32;
/// Space ID (Mission Control desktop)
pub type SpaceID = u64;

pub const K_CG_BACKING_STORE_BUFFERED: i32 = 2;

/// Event types for SLSRegisterNotifyProc (from JankyBorders events.h)
#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum EventType {
    WindowUpdate  = 723,
    WindowClose   = 804,
    WindowMove    = 806,
    WindowResize  = 807,
    WindowReorder = 808,
    WindowLevel   = 811,
    WindowUnhide  = 815,
    WindowHide    = 816,
    WindowTitle   = 1322,
    WindowCreate  = 1325,
    WindowDestroy = 1326,
    SpaceChange   = 1401,
    FrontChange   = 1508,
}

/// Window tags (from JankyBorders window.h)
pub const TAG_DOCUMENT:       u64 = 1 << 0;
pub const TAG_FLOATING:       u64 = 1 << 1;
pub const TAG_ATTACHED:       u64 = 1 << 7;
pub const TAG_STICKY:         u64 = 1 << 11;
pub const TAG_IGNORES_CYCLE:  u64 = 1 << 18;
pub const TAG_MODAL:          u64 = 1 << 31;

/// macOS ProcessSerialNumber
#[repr(C)]
#[derive(Default)]
pub struct ProcessSerialNumber {
    pub high: u32,
    pub low: u32,
}

#[link(name = "SkyLight", kind = "framework")]
extern "C" {
    pub fn SLSMainConnectionID() -> ConnectionID;

    pub fn SLSNewWindow(
        cid: ConnectionID, type_: i32, x: f32, y: f32,
        region: CFTypeRef, wid: *mut WindowID,
    ) -> CGError;
    pub fn SLSReleaseWindow(cid: ConnectionID, wid: WindowID);

    pub fn SLSSetWindowShape(
        cid: ConnectionID, wid: WindowID,
        x: f32, y: f32, region: CFTypeRef,
    ) -> CGError;
    pub fn SLSGetWindowBounds(
        cid: ConnectionID, wid: WindowID, bounds: *mut CGRect,
    ) -> CGError;

    pub fn SLSTransactionCreate(cid: ConnectionID) -> CFTypeRef;
    pub fn SLSTransactionCommit(tx: CFTypeRef, sync: i32);
    pub fn SLSTransactionMoveWindowWithGroup(tx: CFTypeRef, wid: WindowID, origin: CGPoint);
    pub fn SLSTransactionSetWindowLevel(tx: CFTypeRef, wid: WindowID, level: i32);
    pub fn SLSTransactionSetWindowSubLevel(tx: CFTypeRef, wid: WindowID, sub_level: i32);
    pub fn SLSTransactionOrderWindow(tx: CFTypeRef, wid: WindowID, order: i32, rel_wid: WindowID);
    #[allow(dead_code)]
    pub fn SLSTransactionSetWindowTransform(
        tx: CFTypeRef, wid: WindowID, unknown1: i32, unknown2: i32,
        transform: CGAffineTransform,
    );

    #[allow(dead_code)]
    pub fn SLSOrderWindow(cid: ConnectionID, wid: WindowID, mode: i32, relative_wid: WindowID);

    pub fn SLSSetWindowTags(cid: ConnectionID, wid: WindowID, tags: *const u64, mask: i32) -> CGError;
    pub fn SLSClearWindowTags(cid: ConnectionID, wid: WindowID, tags: *const u64, mask: i32) -> CGError;
    pub fn SLSSetWindowResolution(cid: ConnectionID, wid: WindowID, res: f64) -> CGError;
    pub fn SLSSetWindowOpacity(cid: ConnectionID, wid: WindowID, is_opaque: bool) -> CGError;
    #[allow(dead_code)]
    pub fn SLSSetWindowAlpha(cid: ConnectionID, wid: WindowID, alpha: f32) -> CGError;

    pub fn SLSWindowIsOrderedIn(cid: ConnectionID, wid: WindowID, shown: *mut bool) -> CGError;

    pub fn SLSCopyWindowsWithOptionsAndTags(
        cid: ConnectionID, owner: u32, spaces: CFTypeRef, options: u32,
        set_tags: *const u64, clear_tags: *const u64,
    ) -> CFArrayRef;

    pub fn SLSWindowQueryWindows(cid: ConnectionID, windows: CFArrayRef, options: u32) -> CFTypeRef;
    pub fn SLSWindowQueryResultCopyWindows(query: CFTypeRef) -> CFTypeRef;
    pub fn SLSWindowIteratorAdvance(iterator: CFTypeRef) -> bool;
    pub fn SLSWindowIteratorGetWindowID(iterator: CFTypeRef) -> WindowID;
    pub fn SLSWindowIteratorGetCount(iterator: CFTypeRef) -> u32;
    pub fn SLSWindowIteratorGetTags(iterator: CFTypeRef) -> u64;
    pub fn SLSWindowIteratorGetAttributes(iterator: CFTypeRef) -> u64;
    pub fn SLSWindowIteratorGetParentID(iterator: CFTypeRef) -> WindowID;
    pub fn SLSWindowIteratorGetLevel(iterator: CFTypeRef) -> i32;

    pub fn SLSGetWindowOwner(cid: ConnectionID, wid: WindowID, owner_cid: *mut ConnectionID) -> CGError;
    pub fn SLSConnectionGetPID(cid: ConnectionID, pid: *mut libc::pid_t) -> i32;

    pub fn SLSRequestNotificationsForWindows(cid: ConnectionID, wids: *const WindowID, count: u32);

    pub fn _SLPSGetFrontProcess(psn: *mut ProcessSerialNumber) -> i32;
    pub fn SLSGetConnectionIDForPSN(cid: ConnectionID, psn: *const ProcessSerialNumber, out_cid: *mut ConnectionID) -> i32;

    pub fn SLSCopySpacesForWindows(cid: ConnectionID, selector: u32, windows: CFArrayRef) -> CFArrayRef;
    pub fn SLSManagedDisplayGetCurrentSpace(cid: ConnectionID, display: CFStringRef) -> SpaceID;
    pub fn SLSMoveWindowsToManagedSpace(cid: ConnectionID, window_list: CFArrayRef, sid: u64);
    pub fn SLSCopyManagedDisplayForWindow(cid: ConnectionID, wid: WindowID) -> CFStringRef;

    pub fn SLSRegisterNotifyProc(
        callback: extern "C" fn(u32, *mut libc::c_void, usize, *mut libc::c_void),
        event: u32, context: *mut libc::c_void,
    );

    pub fn SLSCopyManagedDisplays(cid: ConnectionID) -> CFArrayRef;
    pub fn SLSCopyManagedDisplaySpaces(cid: ConnectionID) -> CFArrayRef;

    pub fn SLSDisableUpdate(cid: ConnectionID);
    pub fn SLSReenableUpdate(cid: ConnectionID);
    pub fn SLSFlushWindowContentRegion(cid: ConnectionID, wid: WindowID, region: CFTypeRef);

    pub fn SLWindowContextCreate(cid: ConnectionID, wid: WindowID, options: CFTypeRef) -> *mut libc::c_void;

    pub fn SLSGetEventPort(cid: ConnectionID, port: *mut u32) -> CGError;
    pub fn SLEventCreateNextEvent(cid: ConnectionID) -> CFTypeRef;

    pub fn SLSWindowSetShadowProperties(wid: WindowID, properties: CFTypeRef);
}

// ── Corner radii (dlsym'd) ──────────────────────────────────────────────────

type CornerRadiiFn = unsafe extern "C" fn(CFTypeRef) -> CFArrayRef;
static CORNER_RADII_FN: OnceLock<Option<CornerRadiiFn>> = OnceLock::new();

pub fn corner_radii_fn() -> Option<CornerRadiiFn> {
    *CORNER_RADII_FN.get_or_init(|| unsafe {
        let path = b"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight\0";
        let h = libc::dlopen(path.as_ptr() as _, libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if h.is_null() { return None; }
        let s = libc::dlsym(h, b"SLSWindowIteratorGetCornerRadii\0".as_ptr() as _);
        if s.is_null() { None } else { Some(std::mem::transmute(s)) }
    })
}

// ── Safe wrappers ────────────────────────────────────────────────────────────

pub fn main_connection() -> ConnectionID {
    unsafe { SLSMainConnectionID() }
}

pub fn window_is_ordered_in(cid: ConnectionID, wid: WindowID) -> bool {
    let mut shown = false;
    unsafe { SLSWindowIsOrderedIn(cid, wid, &mut shown); }
    shown
}
