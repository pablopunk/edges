//! Border rendering — matches JankyBorders drawing.h exactly.
//!
//! Draw flow from border.c / drawing.h:
//!   1. CGContextClearRect(ctx, frame)
//!   2. Build inner_clip_path (rounded rect inset by 1.0)
//!   3. Clip between full frame and inner_clip_path via even-odd clip
//!   4. Set line width, stroke color
//!   5. Add rounded rect path at drawing_bounds and stroke

use crate::settings::BorderStyle;
use anyhow::Result;
use core_graphics::context::CGContext;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use foreign_types::ForeignTypeRef;
use std::ffi::c_void;

// ── Raw CGPath/CGContext FFI ─────────────────────────────────────────────────

type CGPathRef     = *mut c_void;
type CGMutablePath = *mut c_void;
type NullPtr       = *const c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPathCreateWithRoundedRect(rect: CGRect, rx: f64, ry: f64, t: NullPtr) -> CGPathRef;
    fn CGPathCreateMutable() -> CGMutablePath;
    fn CGPathAddRect(path: CGMutablePath, t: NullPtr, rect: CGRect);
    fn CGPathAddRoundedRect(path: CGMutablePath, t: NullPtr, rect: CGRect, rx: f64, ry: f64);
    fn CGPathAddPath(dst: CGMutablePath, t: NullPtr, src: CGPathRef);
    fn CGPathRelease(p: CGPathRef);

    fn CGContextSaveGState(ctx: *mut c_void);
    fn CGContextRestoreGState(ctx: *mut c_void);
    fn CGContextClearRect(ctx: *mut c_void, rect: CGRect);
    fn CGContextSetLineWidth(ctx: *mut c_void, w: f64);
    fn CGContextSetRGBStrokeColor(ctx: *mut c_void, r: f64, g: f64, b: f64, a: f64);
    fn CGContextSetRGBFillColor(ctx: *mut c_void, r: f64, g: f64, b: f64, a: f64);
    fn CGContextAddPath(ctx: *mut c_void, path: CGPathRef);
    fn CGContextStrokePath(ctx: *mut c_void);
    fn CGContextFillPath(ctx: *mut c_void);
    fn CGContextEOClip(ctx: *mut c_void);
    fn CGContextSetInterpolationQuality(ctx: *mut c_void, quality: i32);
}

const K_CG_INTERPOLATION_NONE: i32 = 0;

// ── Color ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Color { pub r: f64, pub g: f64, pub b: f64, pub a: f64 }

impl Color {
    pub fn from_argb(hex: u32) -> Self {
        Self {
            a: ((hex >> 24) & 0xFF) as f64 / 255.0,
            r: ((hex >> 16) & 0xFF) as f64 / 255.0,
            g: ((hex >> 8)  & 0xFF) as f64 / 255.0,
            b: ( hex        & 0xFF) as f64 / 255.0,
        }
    }
}

// ── Renderer ─────────────────────────────────────────────────────────────────

pub struct BorderRenderer;

impl BorderRenderer {
    pub fn new() -> Result<Self> { Ok(Self) }

    /// Draw a border matching JankyBorders border_draw() exactly.
    ///
    /// `frame`          – border window local rect (origin 0,0)
    /// `drawing_bounds` – target window rect in border-window-local coords
    /// `border_width`   – stroke width (settings.border_width)
    /// `corner_radius`  – from SLSWindowIteratorGetCornerRadii (or 9)
    /// `inner_radius`   – corner_radius + 1 (for clip inset)
    pub fn draw_border(
        &self,
        context:        &CGContext,
        frame:          CGRect,
        drawing_bounds: CGRect,
        border_width:   f64,
        corner_radius:  f64,
        inner_radius:   f64,
        color:          Color,
        style:          BorderStyle,
    ) -> Result<()> {
        let ctx = core_graphics::context::CGContextRef::as_ptr(std::ops::Deref::deref(context)) as *mut c_void;

        unsafe {
            CGContextSaveGState(ctx);
            CGContextSetInterpolationQuality(ctx, K_CG_INTERPOLATION_NONE);

            // 1. Clear
            CGContextClearRect(ctx, frame);

            // 2. Build inner clip path
            let inner_clip_path = CGPathCreateMutable();
            let path_rect = drawing_bounds;

            match style {
                BorderStyle::Square => {
                    CGPathAddRect(inner_clip_path, std::ptr::null(), path_rect);
                }
                _ => {
                    let inset_rect = CGRect {
                        origin: CGPoint {
                            x: path_rect.origin.x + 1.0,
                            y: path_rect.origin.y + 1.0,
                        },
                        size: CGSize {
                            width:  path_rect.size.width  - 2.0,
                            height: path_rect.size.height - 2.0,
                        },
                    };
                    CGPathAddRoundedRect(
                        inner_clip_path, std::ptr::null(), inset_rect,
                        inner_radius, inner_radius,
                    );
                }
            }

            // 3. Clip between frame and inner path (even-odd)
            //    Matching drawing_clip_between_rect_and_path exactly:
            //    Create mutable path, add frame rect, add inner path, EO clip
            let clip_path = CGPathCreateMutable();
            CGPathAddRect(clip_path, std::ptr::null(), frame);
            CGPathAddPath(clip_path, std::ptr::null(), inner_clip_path);
            CGContextAddPath(ctx, clip_path);
            CGContextEOClip(ctx);
            CGPathRelease(clip_path);

            // 4. Set stroke properties
            CGContextSetLineWidth(ctx, border_width);
            CGContextSetRGBStrokeColor(ctx, color.r, color.g, color.b, color.a);
            CGContextSetRGBFillColor(ctx, color.r, color.g, color.b, color.a);

            // 5. Draw the border
            let radius = match style {
                BorderStyle::Square  => 0.0,
                BorderStyle::Uniform => 9.0,
                BorderStyle::Round   => corner_radius,
            };

            match style {
                BorderStyle::Square => {
                    // JB: drawing_draw_square_with_inset(ctx, path_rect, -border_width/2)
                    let inset = -border_width / 2.0;
                    let square_rect = CGRect {
                        origin: CGPoint {
                            x: path_rect.origin.x + inset,
                            y: path_rect.origin.y + inset,
                        },
                        size: CGSize {
                            width:  path_rect.size.width  - 2.0 * inset,
                            height: path_rect.size.height - 2.0 * inset,
                        },
                    };
                    let square_path = CGPathCreateMutable();
                    CGPathAddRect(square_path, std::ptr::null(), square_rect);
                    CGContextAddPath(ctx, square_path);
                    CGPathRelease(square_path);
                    CGContextFillPath(ctx);
                }
                BorderStyle::Uniform => {
                    // JB: first fill at radius 9, then stroke at radius 9
                    let stroke_path = CGPathCreateWithRoundedRect(
                        path_rect, radius, radius, std::ptr::null(),
                    );
                    CGContextAddPath(ctx, stroke_path);
                    CGPathRelease(stroke_path);
                    CGContextFillPath(ctx);

                    let stroke_path2 = CGPathCreateWithRoundedRect(
                        path_rect, radius, radius, std::ptr::null(),
                    );
                    CGContextAddPath(ctx, stroke_path2);
                    CGPathRelease(stroke_path2);
                    CGContextStrokePath(ctx);
                }
                BorderStyle::Round => {
                    // JB: drawing_draw_rounded_rect_with_inset(ctx, path_rect, radius, false)
                    let stroke_path = CGPathCreateWithRoundedRect(
                        path_rect, radius, radius, std::ptr::null(),
                    );
                    CGContextAddPath(ctx, stroke_path);
                    CGPathRelease(stroke_path);
                    CGContextStrokePath(ctx);
                }
            }

            CGPathRelease(inner_clip_path);
            CGContextRestoreGState(ctx);
        }

        Ok(())
    }
}
