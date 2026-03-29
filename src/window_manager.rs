//! Window manager — matches JankyBorders windows.c logic exactly.

use crate::border::BorderWindow;
use core_foundation::base::TCFType;
use crate::ffi::cf;
use crate::ffi::skylight::*;
use crate::settings::Settings;
use anyhow::Result;
use core_foundation::base::CFTypeRef;
use core_graphics::geometry::CGRect;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, trace};

// ── window_suitable (matches JB window.h exactly) ────────────────────────────

fn window_suitable(tags: u64, attributes: u64, parent_wid: u32) -> bool {
    parent_wid == 0
        && ((attributes & 0x2) != 0 || (tags & 0x0400_0000_0000_0000) != 0)
        && (tags & TAG_ATTACHED) == 0
        && (tags & TAG_IGNORES_CYCLE) == 0
        && ((tags & TAG_DOCUMENT) != 0
            || ((tags & TAG_FLOATING) != 0 && (tags & TAG_MODAL) != 0))
}

pub struct WindowManager {
    cid: ConnectionID,
    our_pid: libc::pid_t,
    borders: HashMap<WindowID, BorderWindow>,
    settings: Arc<Settings>,
}

impl WindowManager {
    pub fn new(settings: Arc<Settings>) -> Result<Self> {
        let cid = main_connection();
        if cid == 0 {
            anyhow::bail!("Failed to get window server connection");
        }
        let our_pid = std::process::id() as libc::pid_t;
        debug!(cid, our_pid, "WindowManager created");
        Ok(Self { cid, our_pid, borders: HashMap::new(), settings })
    }

    /// Discover and border all existing windows (matches JB windows_add_existing_windows)
    pub fn add_existing_windows(&mut self) {
        let cid = self.cid;
        let space_list = self.get_all_space_ids();
        if space_list.is_empty() { return; }

        unsafe {
            let space_arr = cf::cfarray_of_u64(&space_list);
            let set_tags: u64 = 1;
            let clear_tags: u64 = 0;
            let window_list = SLSCopyWindowsWithOptionsAndTags(
                cid, 0, space_arr as CFTypeRef, 0x2, &set_tags, &clear_tags,
            );

            if !window_list.is_null() {
                let count = cf::cfarray_count(window_list);
                if count > 0 {
                    let query = SLSWindowQueryWindows(cid, window_list, 0x0);
                    if !query.is_null() {
                        let iter = SLSWindowQueryResultCopyWindows(query);
                        if !iter.is_null() {
                            while SLSWindowIteratorAdvance(iter) {
                                if self.iterator_window_suitable(iter) {
                                    let wid = SLSWindowIteratorGetWindowID(iter);
                                    let sid = self.window_space_id(wid);
                                    self.window_create(wid, sid);
                                }
                            }
                            cf::CFRelease(iter);
                        }
                        cf::CFRelease(query);
                    }
                }
                cf::CFRelease(window_list as CFTypeRef);
            }
            cf::CFRelease(space_arr as CFTypeRef);
        }

        self.update_notifications();
    }

    // ── Event handlers (matching JB events.c dispatch) ───────────────────────

    /// Window created event
    pub fn window_created(&mut self, wid: WindowID, sid: SpaceID) {
        if self.is_own_window(wid) { return; }
        if self.window_create(wid, sid) {
            debug!(wid, sid, "Window created");
            self.determine_and_focus_active_window();
        }
    }

    /// Window destroyed event
    pub fn window_destroyed(&mut self, wid: WindowID, sid: SpaceID) {
        if let Some(border) = self.borders.get(&wid) {
            if border_matches_destroy(border, sid) {
                self.borders.remove(&wid);
                debug!(wid, "Window destroyed");
                self.update_notifications();
            }
        }
        self.determine_and_focus_active_window();
    }

    /// Window move event → fast path (just reposition, no reshape)
    pub fn window_moved(&mut self, wid: WindowID) {
        if let Some(border) = self.borders.get_mut(&wid) {
            border.move_to_target();
        }
    }

    /// Window resize/reorder/level → full update
    pub fn window_updated(&mut self, wid: WindowID) {
        if let Some(border) = self.borders.get_mut(&wid) {
            let _ = border.update();
        }
    }

    /// Window reorder → update + delayed focus
    pub fn window_reordered(&mut self, wid: WindowID) {
        self.window_updated(wid);
        self.determine_and_focus_active_window();
    }

    pub fn window_hidden(&mut self, wid: WindowID) {
        if let Some(border) = self.borders.get(&wid) {
            border.hide();
        }
    }

    pub fn window_unhidden(&mut self, wid: WindowID) {
        if let Some(border) = self.borders.get(&wid) {
            border.unhide();
        }
    }

    /// Window close event → destroy
    pub fn window_closed(&mut self, wid: WindowID) {
        if self.borders.remove(&wid).is_some() {
            debug!(wid, "Window closed");
            self.update_notifications();
        }
    }

    /// Front app changed or title changed → re-detect focus
    pub fn focus_changed(&mut self) {
        self.cleanup_dead_borders();
        self.determine_and_focus_active_window();
    }

    /// Remove borders whose target window no longer exists
    fn cleanup_dead_borders(&mut self) {
        let dead: Vec<WindowID> = self.borders.iter()
            .filter(|(_, b)| !b.target_alive())
            .map(|(wid, _)| *wid)
            .collect();
        for wid in &dead {
            debug!(wid, "Removing ghost border (target dead)");
            self.borders.remove(wid);
        }
        if !dead.is_empty() {
            self.update_notifications();
        }
    }

    /// Space changed → scan current spaces for borders (matches JB windows_draw_borders_on_current_spaces)
    pub fn space_changed(&mut self) {
        self.cleanup_dead_borders();
        let cid = self.cid;
        let current_spaces = self.get_current_space_ids();
        if current_spaces.is_empty() { return; }

        // Collect windows visible on current spaces
        let mut seen_wids = std::collections::HashSet::new();

        unsafe {
            let space_arr = cf::cfarray_of_u64(&current_spaces);
            let set_tags: u64 = 1;
            let clear_tags: u64 = 0;
            let window_list = SLSCopyWindowsWithOptionsAndTags(
                cid, 0, space_arr as CFTypeRef, 0x2, &set_tags, &clear_tags,
            );

            if !window_list.is_null() {
                let query = SLSWindowQueryWindows(cid, window_list, 0x0);
                if !query.is_null() {
                    let iter = SLSWindowQueryResultCopyWindows(query);
                    if !iter.is_null() {
                        while SLSWindowIteratorAdvance(iter) {
                            if self.iterator_window_suitable(iter) {
                                let wid = SLSWindowIteratorGetWindowID(iter);
                                seen_wids.insert(wid);
                                let target_sid = self.window_space_id(wid);
                                if let Some(border) = self.borders.get_mut(&wid) {
                                    // Move border to target's current space if it changed
                                    if target_sid != 0 && target_sid != border.sid() {
                                        border.move_to_space(target_sid);
                                    }
                                    let _ = border.update();
                                } else {
                                    debug!(wid, "Creating missing window on space change");
                                    let sid = self.window_space_id(wid);
                                    self.window_create(wid, sid);
                                }
                            }
                        }
                        cf::CFRelease(iter);
                    }
                    cf::CFRelease(query);
                }
                cf::CFRelease(window_list as CFTypeRef);
            }
            cf::CFRelease(space_arr as CFTypeRef);
        }

        // Hide borders on current spaces whose target windows are no longer here
        // (they moved to another space)
        let stale: Vec<WindowID> = self.borders.iter()
            .filter(|(wid, border)| {
                current_spaces.contains(&border.sid()) && !seen_wids.contains(wid)
            })
            .map(|(wid, _)| *wid)
            .collect();
        for wid in &stale {
            if let Some(border) = self.borders.get(wid) {
                border.hide();
            }
        }
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    /// Try to create a border for wid. Returns true if created. Matches JB windows_window_create.
    fn window_create(&mut self, wid: WindowID, sid: SpaceID) -> bool {
        if self.borders.contains_key(&wid) { return false; }
        if self.is_own_window(wid) { return false; }

        // Check window_suitable via iterator
        let (suitable, radius) = self.check_window_suitable_and_radius(wid);
        if !suitable { return false; }

        // Get target frame
        let mut frame = CGRect::default();
        if unsafe { SLSGetWindowBounds(self.cid, wid, &mut frame) } != 0 {
            return false;
        }

        match BorderWindow::create(self.cid, wid, frame, radius, sid, self.settings.clone()) {
            Ok(mut border) => {
                // Must call update() after creation to position properly
                // (matches JB: border_update(border, false) after create)
                let _ = border.update();
                self.borders.insert(wid, border);
                self.update_notifications();
                true
            }
            Err(e) => {
                trace!(wid, error = %e, "Failed to create border");
                false
            }
        }
    }

    /// Check suitable + get corner radius from iterator (single query)
    fn check_window_suitable_and_radius(&self, wid: WindowID) -> (bool, i32) {
        unsafe {
            let arr = cf::cfarray_of_u32(&[wid]);
            let query = SLSWindowQueryWindows(self.cid, arr, 0x0);
            if query.is_null() { cf::CFRelease(arr as CFTypeRef); return (false, 0); }

            let iter = SLSWindowQueryResultCopyWindows(query);
            if iter.is_null() || SLSWindowIteratorGetCount(iter) == 0
                || !SLSWindowIteratorAdvance(iter)
            {
                if !iter.is_null() { cf::CFRelease(iter); }
                cf::CFRelease(query);
                cf::CFRelease(arr as CFTypeRef);
                return (false, 0);
            }

            let tags = SLSWindowIteratorGetTags(iter);
            let attrs = SLSWindowIteratorGetAttributes(iter);
            let parent = SLSWindowIteratorGetParentID(iter);
            let suitable = window_suitable(tags, attrs, parent);

            let mut radius: i32 = 0;
            if suitable {
                if let Some(f) = corner_radii_fn() {
                    let radii = f(iter);
                    if !radii.is_null() && cf::cfarray_count(radii) > 0 {
                        radius = cf::cfnumber_get_i32(cf::cfarray_get(radii, 0));
                        cf::CFRelease(radii as CFTypeRef);
                    }
                }
            }

            cf::CFRelease(iter);
            cf::CFRelease(query);
            cf::CFRelease(arr as CFTypeRef);
            (suitable, radius)
        }
    }

    /// Check suitable via iterator (for scanning)
    fn iterator_window_suitable(&self, iter: CFTypeRef) -> bool {
        unsafe {
            let tags = SLSWindowIteratorGetTags(iter);
            let attrs = SLSWindowIteratorGetAttributes(iter);
            let parent = SLSWindowIteratorGetParentID(iter);

            if !window_suitable(tags, attrs, parent) { return false; }

            // Check not our own window
            let wid = SLSWindowIteratorGetWindowID(iter);
            !self.is_own_window(wid)
        }
    }

    /// Matches JB windows_determine_and_focus_active_window
    fn determine_and_focus_active_window(&mut self) {
        let front_wid = self.get_front_window();
        if front_wid == 0 { return; }

        // If front window doesn't have a border, create one (slow path)
        let found = self.borders.contains_key(&front_wid);
        if !found {
            debug!(front_wid, "Taking slow window focus path");
            let sid = self.window_space_id(front_wid);
            if sid != 0 {
                self.window_create(front_wid, sid);
            }
        }

        // Update focus on all borders (matching JB windows_window_focus)
        let wids: Vec<WindowID> = self.borders.keys().copied().collect();
        for wid in wids {
            let should_focus = wid == front_wid;
            if let Some(border) = self.borders.get_mut(&wid) {
                if border.is_focused() != should_focus {
                    border.set_focused(should_focus);
                }
            }
        }
    }

    /// Get front window (matches JB get_front_window in window.h)
    fn get_front_window(&self) -> WindowID {
        unsafe {
            let mut psn = ProcessSerialNumber::default();
            if _SLPSGetFrontProcess(&mut psn) != 0 { return 0; }

            let mut target_cid: ConnectionID = 0;
            if SLSGetConnectionIDForPSN(self.cid, &psn, &mut target_cid) != 0 { return 0; }

            let active_sid = self.get_active_space_id();
            if active_sid == 0 { return 0; }

            let space_arr = cf::cfarray_of_u64(&[active_sid]);
            let set_tags: u64 = 1;
            let clear_tags: u64 = 0;
            let window_list = SLSCopyWindowsWithOptionsAndTags(
                self.cid, target_cid as u32, space_arr as CFTypeRef,
                0x2, &set_tags, &clear_tags,
            );
            cf::CFRelease(space_arr as CFTypeRef);

            if window_list.is_null() { return 0; }

            let mut front_wid: WindowID = 0;
            let query = SLSWindowQueryWindows(self.cid, window_list, 0x0);
            if !query.is_null() {
                let iter = SLSWindowQueryResultCopyWindows(query);
                if !iter.is_null() && SLSWindowIteratorGetCount(iter) > 0 {
                    while SLSWindowIteratorAdvance(iter) {
                        let tags = SLSWindowIteratorGetTags(iter);
                        let attrs = SLSWindowIteratorGetAttributes(iter);
                        let parent = SLSWindowIteratorGetParentID(iter);
                        if window_suitable(tags, attrs, parent) {
                            front_wid = SLSWindowIteratorGetWindowID(iter);
                            break;
                        }
                    }
                }
                if !iter.is_null() { cf::CFRelease(iter); }
                cf::CFRelease(query);
            }
            cf::CFRelease(window_list as CFTypeRef);
            front_wid
        }
    }

    fn is_own_window(&self, wid: WindowID) -> bool {
        // Check border windows
        if self.borders.values().any(|b| b.wid() == wid) { return true; }
        unsafe {
            let mut owner_cid: ConnectionID = 0;
            if SLSGetWindowOwner(self.cid, wid, &mut owner_cid) != 0 { return false; }
            let mut pid: libc::pid_t = 0;
            if SLSConnectionGetPID(owner_cid, &mut pid) != 0 { return false; }
            pid == self.our_pid
        }
    }

    fn update_notifications(&self) {
        let wids: Vec<WindowID> = self.borders.keys().copied().collect();
        if wids.is_empty() { return; }
        unsafe {
            SLSRequestNotificationsForWindows(self.cid, wids.as_ptr(), wids.len() as u32);
        }
    }

    /// Get active space ID — uses the active menu bar display (correct for multi-monitor).
    /// Matches JB get_active_space_id from space.h.
    fn get_active_space_id(&self) -> SpaceID {
        unsafe {
            let uuid_ref = SLSCopyActiveMenuBarDisplayIdentifier(self.cid);
            if uuid_ref.is_null() {
                // Fallback: single display
                let displays = SLSCopyManagedDisplays(self.cid);
                if displays.is_null() { return 0; }
                let count = cf::cfarray_count(displays);
                if count == 0 { cf::CFRelease(displays as CFTypeRef); return 0; }
                let uuid = cf::cfarray_get(displays, 0);
                let sid = SLSManagedDisplayGetCurrentSpace(self.cid, uuid as _);
                cf::CFRelease(displays as CFTypeRef);
                return sid;
            }
            let sid = SLSManagedDisplayGetCurrentSpace(self.cid, uuid_ref);
            cf::CFRelease(uuid_ref as CFTypeRef);
            sid
        }
    }

    /// Get current space IDs for all displays
    fn get_current_space_ids(&self) -> Vec<SpaceID> {
        let mut spaces = Vec::new();
        unsafe {
            let displays = SLSCopyManagedDisplays(self.cid);
            if displays.is_null() { return spaces; }
            let count = cf::cfarray_count(displays);
            for i in 0..count {
                let uuid = cf::cfarray_get(displays, i);
                spaces.push(SLSManagedDisplayGetCurrentSpace(self.cid, uuid as _));
            }
            cf::CFRelease(displays as CFTypeRef);
        }
        spaces
    }

    /// Get all space IDs across all displays (matches JB windows_add_existing_windows)
    fn get_all_space_ids(&self) -> Vec<SpaceID> {
        let mut spaces = Vec::new();
        unsafe {
            let display_spaces = SLSCopyManagedDisplaySpaces(self.cid);
            if display_spaces.is_null() { return spaces; }
            let dc = cf::cfarray_count(display_spaces);
            for i in 0..dc {
                let display_dict = cf::cfarray_get(display_spaces, i);
                // Get "Spaces" key from dict
                let key = core_foundation::string::CFString::new("Spaces");
                let spaces_arr: CFTypeRef;
                extern "C" {
                    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
                }
                spaces_arr = CFDictionaryGetValue(display_dict, key.as_concrete_TypeRef() as CFTypeRef);
                if !spaces_arr.is_null() {
                    let sc = cf::cfarray_count(spaces_arr as _);
                    for j in 0..sc {
                        let space_dict = cf::cfarray_get(spaces_arr as _, j);
                        let id_key = core_foundation::string::CFString::new("id64");
                        let id_ref = CFDictionaryGetValue(space_dict, id_key.as_concrete_TypeRef() as CFTypeRef);
                        if !id_ref.is_null() {
                            spaces.push(cf::cfnumber_get_u64(id_ref));
                        }
                    }
                }
            }
            cf::CFRelease(display_spaces as CFTypeRef);
        }
        spaces
    }

    /// Get space ID for a window (matches JB window_space_id)
    fn window_space_id(&self, wid: WindowID) -> SpaceID {
        unsafe {
            let arr = cf::cfarray_of_u32(&[wid]);
            let space_list = SLSCopySpacesForWindows(self.cid, 0x7, arr);
            let mut sid: SpaceID = 0;
            if !space_list.is_null() {
                let count = cf::cfarray_count(space_list);
                if count > 0 {
                    sid = cf::cfnumber_get_u64(cf::cfarray_get(space_list, 0));
                }
                cf::CFRelease(space_list as CFTypeRef);
            }
            cf::CFRelease(arr as CFTypeRef);

            if sid != 0 { return sid; }

            // Fallback: use display's current space
            let uuid = SLSCopyManagedDisplayForWindow(self.cid, wid);
            if !uuid.is_null() {
                sid = SLSManagedDisplayGetCurrentSpace(self.cid, uuid);
                cf::CFRelease(uuid as CFTypeRef);
            }
            sid
        }
    }
}

/// Check if border matches destroy event (JB checks sid match or sticky)
fn border_matches_destroy(_border: &BorderWindow, _sid: SpaceID) -> bool {
    // JB: border->sid == sid || border->sticky || sid == 0
    // For simplicity, always allow destroy
    true
}
