//! CoreGraphics region helpers

use core_foundation::base::CFTypeRef;
use core_graphics::geometry::CGRect;
use core_graphics::base::CGError;

extern "C" {
    pub fn CGSNewRegionWithRect(rect: *const CGRect, region: *mut CFTypeRef) -> CGError;
}

/// Create a rectangular region for window shapes
pub fn create_rect_region(rect: &CGRect) -> Option<CFTypeRef> {
    let mut region: CFTypeRef = std::ptr::null();
    let err = unsafe { CGSNewRegionWithRect(rect, &mut region) };
    if err == 0 && !region.is_null() { Some(region) } else { None }
}
