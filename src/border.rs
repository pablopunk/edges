//! Individual border window — matches JankyBorders border.c logic exactly.

use crate::ffi::cf;
use crate::ffi::cg;
use crate::ffi::skylight::*;
use crate::renderer::{BorderRenderer, Color};
use crate::settings::{BorderOrder, ColorSpec, Settings};
use core_foundation::base::TCFType;
use anyhow::Result;
use core_foundation::base::CFTypeRef;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use foreign_types::ForeignType;
use std::ffi::c_void;
use std::sync::Arc;
use tracing::{debug, trace};

/// BORDER_PADDING from JankyBorders border.h
const BORDER_PADDING: f64 = 8.0;

// Shadow disable
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDictionaryCreate(
        alloc: *const c_void, keys: *const CFTypeRef, values: *const CFTypeRef,
        count: isize, key_cbs: *const c_void, val_cbs: *const c_void,
    ) -> CFTypeRef;
    fn CFNumberCreate(alloc: *const c_void, t: i32, v: *const c_void) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

fn frames_equal(a: CGRect, b: CGRect) -> bool {
    (a.origin.x - b.origin.x).abs() < 0.01
        && (a.origin.y - b.origin.y).abs() < 0.01
        && (a.size.width - b.size.width).abs() < 0.01
        && (a.size.height - b.size.height).abs() < 0.01
}

/// Disable shadow on a window (matches JB window_create)
unsafe fn disable_window_shadow(wid: WindowID) {
    let density: isize = 0;
    let density_cf = CFNumberCreate(
        std::ptr::null(), 22, // kCFNumberCFIndexType
        &density as *const isize as *const c_void,
    );

    // CFSTR("com.apple.WindowShadowDensity")
    let key_bytes = b"com.apple.WindowShadowDensity\0";
    let key = core_foundation::string::CFString::new(
        std::str::from_utf8_unchecked(&key_bytes[..key_bytes.len()-1])
    );
    let key_ref: CFTypeRef = key.as_concrete_TypeRef() as CFTypeRef;

    let keys = [key_ref];
    let values = [density_cf];
    let dict = CFDictionaryCreate(
        std::ptr::null(), keys.as_ptr(), values.as_ptr(), 1,
        &kCFTypeDictionaryKeyCallBacks as *const c_void,
        &kCFTypeDictionaryValueCallBacks as *const c_void,
    );
    SLSWindowSetShadowProperties(wid, dict);
    CFRelease(density_cf);
    CFRelease(dict);
}

/// Represents a single border overlay window
pub struct BorderWindow {
    wid: WindowID,
    target_wid: WindowID,
    cid: ConnectionID,
    /// The border window's frame (local coords, origin=0,0)
    frame: CGRect,
    /// The screen-space origin where border is placed
    origin: CGPoint,
    /// Target window's bounds
    target_bounds: CGRect,
    /// Drawing bounds: target rect in border-window local coords
    drawing_bounds: CGRect,
    /// Corner radius from window server
    radius: f64,
    /// inner_radius = radius + 1 (for clip inset)
    inner_radius: f64,
    /// Space ID the target is on
    #[allow(dead_code)]
    sid: SpaceID,
    /// Whether target is sticky (visible on all spaces)
    sticky: bool,
    focused: bool,
    needs_redraw: bool,
    too_small: bool,
    settings: Arc<Settings>,
}

impl BorderWindow {
    /// Create a new border window for the given target. Matches JB windows_window_create + border_create_window.
    pub fn create(
        cid: ConnectionID,
        target_wid: WindowID,
        target_frame: CGRect,
        radius: i32,
        sid: SpaceID,
        settings: Arc<Settings>,
    ) -> Result<Self> {
        let radius = if radius > 0 { radius as f64 } else { 9.0 };
        let inner_radius = radius + 1.0;

        // Calculate border frame (same as border_calculate_bounds)
        let border_width = settings.width as f64;
        let border_offset = -border_width - BORDER_PADDING;
        let frame = cgrect_inset(target_frame, border_offset);
        let origin = frame.origin;
        let local_frame = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: frame.size,
        };
        let drawing_bounds = CGRect {
            origin: CGPoint { x: -border_offset, y: -border_offset },
            size: target_frame.size,
        };

        // Check too_small
        let smallest = cgrect_inset(target_frame, 1.0);
        let too_small = smallest.size.width < 2.0 * inner_radius
            || smallest.size.height < 2.0 * inner_radius;

        // Create region from local_frame (matching JB: region is frame-sized)
        let region = cg::create_rect_region(&local_frame)
            .ok_or_else(|| anyhow::anyhow!("Failed to create region"))?;

        // SLSNewWindow
        let wid = unsafe {
            let mut wid: WindowID = 0;
            let err = SLSNewWindow(cid, K_CG_BACKING_STORE_BUFFERED, -9999.0, -9999.0, region, &mut wid);
            cf::CFRelease(region);
            if err != 0 {
                anyhow::bail!("SLSNewWindow failed: {}", err);
            }
            wid
        };

        // Configure window (matching JB window_create exactly)
        unsafe {
            SLSSetWindowResolution(cid, wid, if settings.hidpi { 2.0 } else { 1.0 });
            let set_tags: u64 = (1 << 1) | (1 << 9);
            let clear_tags: u64 = 0;
            SLSSetWindowTags(cid, wid, &set_tags, 64);
            SLSClearWindowTags(cid, wid, &clear_tags, 64);
            SLSSetWindowOpacity(cid, wid, false);
            SLSSetWindowAlpha(cid, wid, 1.0);
            disable_window_shadow(wid);
        }

        // Create CGContext (matching JB: border->context = SLWindowContextCreate)
        // The context is created per-draw in our case (or we could cache it)

        // Send border window to same space as target
        unsafe {
            let arr = cf::cfarray_of_u32(&[wid]);
            SLSMoveWindowsToManagedSpace(cid, arr, sid);
            cf::CFRelease(arr as CFTypeRef);
        }

        debug!(wid, target_wid, radius, "Border window created");

        let mut border = Self {
            wid, target_wid, cid,
            frame: local_frame,
            origin,
            target_bounds: target_frame,
            drawing_bounds,
            radius,
            inner_radius,
            sid,
            sticky: false,
            focused: false,
            needs_redraw: true,
            too_small,
            settings,
        };

        if !too_small {
            border.draw()?;
        }

        Ok(border)
    }

    pub fn wid(&self) -> WindowID { self.wid }
    pub fn is_focused(&self) -> bool { self.focused }

    /// Check if target window still exists
    pub fn target_alive(&self) -> bool {
        let mut owner: ConnectionID = 0;
        unsafe { SLSGetWindowOwner(self.cid, self.target_wid, &mut owner) == 0 }
    }

    /// Get the space ID this border is on
    pub fn sid(&self) -> SpaceID { self.sid }

    /// Move the border window to a new space
    pub fn move_to_space(&mut self, new_sid: SpaceID) {
        if new_sid == 0 || new_sid == self.sid { return; }
        self.sid = new_sid;
        unsafe {
            let arr = cf::cfarray_of_u32(&[self.wid]);
            SLSMoveWindowsToManagedSpace(self.cid, arr, new_sid);
            cf::CFRelease(arr as CFTypeRef);
        }
        self.needs_redraw = true;
    }

    /// Full update — matches JB border_update_internal.
    pub fn update(&mut self) -> Result<()> {
        let cid = self.cid;
        let target_wid = self.target_wid;
        let settings = self.settings.clone();

        // Get current target bounds — if this fails, target is gone
        let mut target_frame = CGRect::default();
        if unsafe { SLSGetWindowBounds(cid, target_wid, &mut target_frame) } != 0 {
            return Ok(());
        }
        self.target_bounds = target_frame;

        // Check too_small
        let smallest = cgrect_inset(target_frame, 1.0);
        self.too_small = smallest.size.width < 2.0 * self.inner_radius
            || smallest.size.height < 2.0 * self.inner_radius;
        if self.too_small {
            self.hide();
            return Ok(());
        }

        // Check tags for sticky
        let tags = query_window_tags(cid, target_wid);
        self.sticky = (tags & TAG_STICKY) != 0;

        // Check if target is visible
        let ordered_in = window_is_ordered_in(cid, target_wid);
        if !ordered_in {
            self.hide();
            return Ok(());
        }

        // Get target's level and sublevel
        let level = query_window_level(cid, target_wid);
        let sub_level = query_window_sub_level(cid, target_wid);

        // Calculate new border frame
        let border_width = settings.width as f64;
        let border_offset = -border_width - BORDER_PADDING;
        let new_full_frame = cgrect_inset(target_frame, border_offset);
        let new_origin = new_full_frame.origin;
        let new_frame = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: new_full_frame.size,
        };
        let new_drawing_bounds = CGRect {
            origin: CGPoint { x: -border_offset, y: -border_offset },
            size: target_frame.size,
        };

        // Check if frame changed → need reshape
        let mut disabled_update = false;
        if !frames_equal(new_frame, self.frame) {
            unsafe {
                let tx = SLSTransactionCreate(cid);
                if tx.is_null() { return Ok(()); }
                disabled_update = true;
                SLSDisableUpdate(cid);

                // Reshape the window (matching JB: SLSSetWindowShape with new region)
                if let Some(region) = cg::create_rect_region(&new_frame) {
                    SLSSetWindowShape(cid, self.wid, -9999.0, -9999.0, region);
                    cf::CFRelease(region);
                }

                self.needs_redraw = true;
                self.frame = new_frame;
                self.drawing_bounds = new_drawing_bounds;

                SLSTransactionOrderWindow(tx, self.wid, 0, target_wid);
                SLSTransactionCommit(tx, 0);
                cf::CFRelease(tx);
            }
        }

        // Draw if needed
        if self.needs_redraw {
            self.drawing_bounds = new_drawing_bounds;
            self.draw()?;
        }

        // Position + order transaction (matching JB border_update_internal)
        unsafe {
            let tx = SLSTransactionCreate(cid);
            if !tx.is_null() {
                SLSTransactionMoveWindowWithGroup(tx, self.wid, new_origin);

                // JB applies SLSTransactionSetWindowTransform with -origin here,
                // but on some macOS versions this pushes the window off-screen.
                // TODO: investigate if needed on macOS 26+

                SLSTransactionSetWindowLevel(tx, self.wid, level);
                SLSTransactionSetWindowSubLevel(tx, self.wid, sub_level);

                let order = match settings.border_order {
                    BorderOrder::Above => 1,
                    BorderOrder::Below => -1,
                };
                SLSTransactionOrderWindow(tx, self.wid, order, target_wid);
                SLSTransactionCommit(tx, 0);
                cf::CFRelease(tx);
            }
        }

        // Update tags (sticky propagation)
        unsafe {
            let mut set_tags: u64 = (1 << 1) | (1 << 9);
            let mut clear_tags: u64 = 0;
            if self.sticky {
                set_tags |= TAG_STICKY;
                clear_tags |= 1 << 45;
            }
            SLSSetWindowTags(cid, self.wid, &set_tags, 0x40);
            SLSClearWindowTags(cid, self.wid, &clear_tags, 0x40);
        }

        self.origin = new_origin;

        if disabled_update {
            unsafe { SLSReenableUpdate(cid); }
        }

        Ok(())
    }

    /// Fast move — matches JB border_move() (no reshape/redraw)
    pub fn move_to_target(&mut self) {
        let cid = self.cid;
        let mut target_frame = CGRect::default();
        if unsafe { SLSGetWindowBounds(cid, self.target_wid, &mut target_frame) } != 0 {
            return;
        }

        let border_width = self.settings.width as f64;
        let offset = border_width + BORDER_PADDING;
        let origin = CGPoint {
            x: target_frame.origin.x - offset,
            y: target_frame.origin.y - offset,
        };

        unsafe {
            let tx = SLSTransactionCreate(cid);
            if !tx.is_null() {
                SLSTransactionMoveWindowWithGroup(tx, self.wid, origin);
                SLSTransactionCommit(tx, 0);
                cf::CFRelease(tx);
            }
        }

        self.target_bounds = target_frame;
        self.origin = origin;
    }

    /// Draw border content — matches JB border_draw()
    fn draw(&mut self) -> Result<()> {
        if self.too_small { return Ok(()); }
        self.needs_redraw = false;

        let color = self.current_color();

        let ctx_ptr = unsafe { SLWindowContextCreate(self.cid, self.wid, std::ptr::null()) };
        if ctx_ptr.is_null() {
            anyhow::bail!("SLWindowContextCreate returned null");
        }
        let context = unsafe { core_graphics::context::CGContext::from_ptr(ctx_ptr as *mut _) };

        let renderer = BorderRenderer::new()?;
        renderer.draw_border(
            &context,
            self.frame,
            self.drawing_bounds,
            self.settings.width as f64,
            self.radius,
            self.inner_radius,
            color,
            self.settings.style,
        )?;

        context.flush();
        std::mem::forget(context); // SkyLight owns the context

        unsafe {
            SLSFlushWindowContentRegion(self.cid, self.wid, std::ptr::null());
        }

        trace!(wid = self.wid, "Border drawn");
        Ok(())
    }

    fn current_color(&self) -> Color {
        let spec = if self.focused {
            &self.settings.colors.active
        } else {
            &self.settings.colors.inactive
        };
        match spec {
            ColorSpec::Solid { color } => Color::from_argb(*color),
            ColorSpec::Gradient { start, .. } => Color::from_argb(*start),
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        if self.focused != focused {
            self.focused = focused;
            self.needs_redraw = true;
            let _ = self.update();
        }
    }

    /// Hide — matches JB border_hide()
    pub fn hide(&self) {
        unsafe {
            let tx = SLSTransactionCreate(self.cid);
            if !tx.is_null() {
                SLSTransactionOrderWindow(tx, self.wid, 0, self.target_wid);
                SLSTransactionCommit(tx, 0);
                cf::CFRelease(tx);
            }
        }
    }

    /// Unhide — matches JB border_unhide()
    pub fn unhide(&self) {
        if self.too_small { return; }
        unsafe {
            let tx = SLSTransactionCreate(self.cid);
            if !tx.is_null() {
                let order = match self.settings.border_order {
                    BorderOrder::Above => 1,
                    BorderOrder::Below => -1,
                };
                SLSTransactionOrderWindow(tx, self.wid, order, self.target_wid);
                SLSTransactionCommit(tx, 0);
                cf::CFRelease(tx);
            }
        }
    }
}

impl Drop for BorderWindow {
    fn drop(&mut self) {
        trace!(wid = self.wid, "Destroying border window");
        unsafe { SLSReleaseWindow(self.cid, self.wid); }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Matches CGRectInset: positive d shrinks, negative d expands
fn cgrect_inset(r: CGRect, d: f64) -> CGRect {
    CGRect {
        origin: CGPoint { x: r.origin.x + d, y: r.origin.y + d },
        size: CGSize {
            width:  r.size.width  - 2.0 * d,
            height: r.size.height - 2.0 * d,
        },
    }
}

/// Query window tags via SLSWindowQueryWindows iterator
pub fn query_window_tags(cid: ConnectionID, wid: WindowID) -> u64 {
    unsafe {
        let arr = cf::cfarray_of_u32(&[wid]);
        let query = SLSWindowQueryWindows(cid, arr, 0x0);
        if query.is_null() { cf::CFRelease(arr as CFTypeRef); return 0; }
        let iter = SLSWindowQueryResultCopyWindows(query);
        let tags = if !iter.is_null()
            && SLSWindowIteratorGetCount(iter) > 0
            && SLSWindowIteratorAdvance(iter)
        {
            SLSWindowIteratorGetTags(iter)
        } else { 0 };
        if !iter.is_null() { cf::CFRelease(iter); }
        cf::CFRelease(query);
        cf::CFRelease(arr as CFTypeRef);
        tags
    }
}

/// Query window level via iterator (matching JB window_level)
pub fn query_window_level(cid: ConnectionID, wid: WindowID) -> i32 {
    unsafe {
        let arr = cf::cfarray_of_u32(&[wid]);
        let query = SLSWindowQueryWindows(cid, arr, 0x0);
        let mut level = 0i32;
        if !query.is_null() {
            let iter = SLSWindowQueryResultCopyWindows(query);
            if !iter.is_null() && SLSWindowIteratorAdvance(iter) {
                level = SLSWindowIteratorGetLevel(iter);
            }
            if !iter.is_null() { cf::CFRelease(iter); }
            cf::CFRelease(query);
        }
        cf::CFRelease(arr as CFTypeRef);
        level
    }
}

/// Query window sub-level. JB uses raw mach messages for this, but for simplicity
/// we'll use the iterator approach or default to 0.
pub fn query_window_sub_level(_cid: ConnectionID, _wid: WindowID) -> i32 {
    // TODO: implement mach message approach like JB for exact sub_level
    // For now return 0 which works for most windows
    0
}

/// Query corner radius from iterator
#[allow(dead_code)]
pub fn query_corner_radius(cid: ConnectionID, wid: WindowID) -> i32 {
    unsafe {
        let arr = cf::cfarray_of_u32(&[wid]);
        let query = SLSWindowQueryWindows(cid, arr, 0x0);
        let mut radius: i32 = 0;
        if !query.is_null() {
            let iter = SLSWindowQueryResultCopyWindows(query);
            if !iter.is_null()
                && SLSWindowIteratorGetCount(iter) > 0
                && SLSWindowIteratorAdvance(iter)
            {
                if let Some(f) = corner_radii_fn() {
                    let radii = f(iter);
                    if !radii.is_null() && cf::cfarray_count(radii) > 0 {
                        radius = cf::cfnumber_get_i32(cf::cfarray_get(radii, 0));
                        cf::CFRelease(radii as CFTypeRef);
                    }
                }
            }
            if !iter.is_null() { cf::CFRelease(iter); }
            cf::CFRelease(query);
        }
        cf::CFRelease(arr as CFTypeRef);
        radius
    }
}
