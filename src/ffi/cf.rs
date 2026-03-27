//! CoreFoundation helpers matching JankyBorders' cfarray_of_cfnumbers

use core_foundation::base::CFTypeRef;
use core_foundation::array::CFArrayRef;
use std::ffi::c_void;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayCreate(
        alloc: *const c_void, values: *const CFTypeRef,
        count: isize, cbs: *const c_void,
    ) -> CFArrayRef;
    fn CFArrayGetCount(a: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFArrayRef, i: isize) -> CFTypeRef;
    fn CFNumberCreate(alloc: *const c_void, t: i32, v: *const c_void) -> CFTypeRef;
    fn CFNumberGetValue(n: CFTypeRef, t: i32, out: *mut c_void) -> bool;
    pub fn CFRelease(cf: CFTypeRef);

    // kCFTypeArrayCallBacks
    static kCFTypeArrayCallBacks: c_void;
}

pub const CF_NUMBER_SINT32: i32 = 3;   // kCFNumberSInt32Type
pub const CF_NUMBER_SINT64: i32 = 4;   // kCFNumberSInt64Type

/// Create a CFArray of CFNumbers from a slice of u32 values
pub unsafe fn cfarray_of_u32(values: &[u32]) -> CFArrayRef {
    let nums: Vec<CFTypeRef> = values.iter().map(|v| {
        CFNumberCreate(std::ptr::null(), CF_NUMBER_SINT32, v as *const u32 as *const c_void)
    }).collect();
    let arr = CFArrayCreate(
        std::ptr::null(), nums.as_ptr(), nums.len() as isize,
        &kCFTypeArrayCallBacks as *const c_void,
    );
    for n in &nums { CFRelease(*n); }
    arr
}

/// Create a CFArray of CFNumbers from a slice of u64 values
pub unsafe fn cfarray_of_u64(values: &[u64]) -> CFArrayRef {
    let nums: Vec<CFTypeRef> = values.iter().map(|v| {
        CFNumberCreate(std::ptr::null(), CF_NUMBER_SINT64, v as *const u64 as *const c_void)
    }).collect();
    let arr = CFArrayCreate(
        std::ptr::null(), nums.as_ptr(), nums.len() as isize,
        &kCFTypeArrayCallBacks as *const c_void,
    );
    for n in &nums { CFRelease(*n); }
    arr
}

pub unsafe fn cfarray_count(arr: CFArrayRef) -> isize {
    CFArrayGetCount(arr)
}

pub unsafe fn cfarray_get(arr: CFArrayRef, idx: isize) -> CFTypeRef {
    CFArrayGetValueAtIndex(arr, idx)
}

pub unsafe fn cfnumber_get_i32(num: CFTypeRef) -> i32 {
    let mut v: i32 = 0;
    CFNumberGetValue(num, CF_NUMBER_SINT32, &mut v as *mut i32 as *mut c_void);
    v
}

pub unsafe fn cfnumber_get_u64(num: CFTypeRef) -> u64 {
    let mut v: u64 = 0;
    CFNumberGetValue(num, CF_NUMBER_SINT64, &mut v as *mut u64 as *mut c_void);
    v
}
