//! # Mandelbrot Set Renderer (Rust + WebAssembly)
//!
//! This crate computes pixels for the **Mandelbrot set** and exports a function
//! to JavaScript via `wasm-bindgen` so a browser can render the fractal onto an
//! HTML5 `<canvas>`.
//!
//! ## What is the Mandelbrot set?
//! Every pixel on the screen represents a **complex number** `c = cx + cy * i`,
//! where `cx` is the horizontal (real) coordinate and `cy` is the vertical
//! (imaginary) coordinate.
//!
//! Starting from `z = 0`, we repeatedly apply the formula:
//!
//! ```text
//! z = z² + c
//! ```
//!
//! - If `z` stays bounded forever (never flies off toward infinity), `c` is
//!   **inside** the Mandelbrot set and is colored **black**.
//! - If `|z|` grows past `2` (we use a larger bailout radius of `16` for smoother
//!   coloring), `c` is **outside** the set. How quickly it escapes determines
//!   its color.

use std::f64::consts::LN_2;
use wasm_bindgen::prelude::*;

/// The complex-plane span covered by the shorter canvas edge at `zoom = 1.0`.
///
/// The Mandelbrot set sits roughly inside `x ∈ [-2.0, 0.47]` and `y ∈ [-1.12, 1.12]`.
/// A base span of `3.2` centered at `(-0.5, 0.0)` frames the entire fractal with
/// comfortable margins on both square and widescreen canvases.
const BASE_SPAN: f64 = 3.2;

/// Squared bailout radius (`16² = 256`).
///
/// Mathematically, any orbit with `|z| > 2` (`|z|² > 4`) is guaranteed to escape
/// to infinity. Using a slightly larger bailout (`|z|² > 256`) makes the smooth
/// coloring formula much more accurate and eliminates visible color bands.
const BAILOUT_SQ: f64 = 256.0;

/// Fast geometric check for the two largest regions inside the Mandelbrot set:
/// the **main cardioid** and the **period-2 bulb** (the big circle to its left).
///
/// Points in either region are mathematically guaranteed never to escape, so we
/// can skip the iteration loop entirely for a big speedup.
#[inline]
fn is_in_cardioid_or_bulb(cx: f64, cy: f64) -> bool {
    let cy2 = cy * cy;

    // Period-2 bulb: circle of radius 1/4 centered at (-1, 0).
    // Equation: (cx + 1)² + cy² <= (1/4)² = 0.0625
    let p2_x = cx + 1.0;
    if p2_x * p2_x + cy2 <= 0.0625 {
        return true;
    }

    // Main cardioid check:
    // Let q = (cx - 1/4)² + cy². Point is inside iff q * (q + (cx - 1/4)) <= 1/4 * cy².
    let x_shift = cx - 0.25;
    let q = x_shift * x_shift + cy2;
    q * (q + x_shift) <= 0.25 * cy2
}

/// Computes the **smooth escape iteration count** for a complex point `c = (cx, cy)`.
///
/// In Rust, `Option<f64>` represents a value that may or may not exist:
/// - `None` means the point did **not** escape within `max_iter` steps (inside the set).
/// - `Some(smooth_iter)` means the point escaped, returning a fractional iteration
///   count suitable for smooth gradient coloring.
pub fn escape_time(cx: f64, cy: f64, max_iter: u32) -> Option<f64> {
    if max_iter == 0 {
        return None;
    }

    if is_in_cardioid_or_bulb(cx, cy) {
        return None;
    }

    // Start at z = 0 + 0i.
    let mut zx = 0.0_f64;
    let mut zy = 0.0_f64;

    for iter in 0..max_iter {
        let zx2 = zx * zx;
        let zy2 = zy * zy;
        let mag_sq = zx2 + zy2;

        // Did |z| exceed our bailout radius?
        if mag_sq > BAILOUT_SQ {
            // Smooth (normalized) iteration count:
            //   nu = iter + 1 - log2(log2(|z|))
            // Because `mag_sq = |z|²`, we have `ln(|z|) = 0.5 * ln(mag_sq)`.
            let log_zn = 0.5 * mag_sq.ln();
            let nu = (log_zn / LN_2).ln() / LN_2;
            let smooth = (iter as f64) + 1.0 - nu;
            return Some(smooth.max(0.0));
        }

        // Complex multiplication:
        //   (zx + zy*i)² + (cx + cy*i)
        // = (zx² - zy² + cx) + (2*zx*zy + cy)*i
        zy = 2.0 * zx * zy + cy;
        zx = zx2 - zy2 + cx;
    }

    // Reached `max_iter` without escaping: treat as inside the Mandelbrot set.
    None
}

/// Maps an optional smooth escape value into an `[R, G, B, A]` pixel color.
///
/// - Points inside the set (`None`) are rendered as solid black `[0, 0, 0, 255]`.
/// - Escaped points (`Some(smooth_iter)`) are mapped smoothly through a 5-stop
///   cyclic gradient (deep midnight blue -> electric blue -> icy cyan -> gold ->
///   cosmic purple -> back to midnight blue).
pub fn smooth_color(escape: Option<f64>) -> [u8; 4] {
    let Some(smooth_iter) = escape else {
        // Inside the Mandelbrot set: opaque black (alpha = 255).
        return [0, 0, 0, 255];
    };

    // Palette stops: (position in [0.0, 1.0], (R, G, B)).
    // The final stop at 1.0 matches 0.0 so the palette wraps around seamlessly
    // as you zoom deeper and iteration counts grow.
    const STOPS: [(f64, [f64; 3]); 6] = [
        (0.0000, [0.0, 7.0, 100.0]),     // Deep midnight blue
        (0.1600, [32.0, 107.0, 203.0]),  // Electric royal blue
        (0.4200, [237.0, 255.0, 255.0]), // Bright icy cyan-white
        (0.6425, [255.0, 170.0, 0.0]),   // Vivid golden amber
        (0.8575, [48.0, 12.0, 68.0]),    // Deep cosmic purple
        (1.0000, [0.0, 7.0, 100.0]),     // Wrap smoothly back to midnight blue
    ];

    // Divide by 50.0 to control how fast the palette cycles, then take the
    // fractional part `t ∈ [0.0, 1.0)` so colors repeat smoothly at high zoom.
    let t = (smooth_iter / 50.0).rem_euclid(1.0);

    // Find the two adjacent palette stops that bracket `t` and linearly
    // interpolate between them.
    for window in STOPS.windows(2) {
        let (p0, c0) = window[0];
        let (p1, c1) = window[1];

        if t >= p0 && t <= p1 {
            // `local_t` goes from 0.0 at stop `p0` to 1.0 at stop `p1`.
            let local_t = (t - p0) / (p1 - p0);
            // Smoothstep eases the transition so there are no sharp corners
            // where two gradient segments meet.
            let s = local_t * local_t * (3.0 - 2.0 * local_t);

            let r = (c0[0] + (c1[0] - c0[0]) * s).round().clamp(0.0, 255.0) as u8;
            let g = (c0[1] + (c1[1] - c0[1]) * s).round().clamp(0.0, 255.0) as u8;
            let b = (c0[2] + (c1[2] - c0[2]) * s).round().clamp(0.0, 255.0) as u8;
            return [r, g, b, 255];
        }
    }

    // Fallback (unreachable for finite `t`, kept for safety).
    [0, 7, 100, 255]
}

/// Renders the Mandelbrot set into a flat RGBA pixel buffer for HTML5 Canvas.
///
/// The `#[wasm_bindgen]` attribute tells `wasm-bindgen` to expose this Rust
/// function to JavaScript. In JS it returns a `Uint8Array` of length
/// `width * height * 4`, where every 4 consecutive bytes are `[R, G, B, A]`
/// for one pixel, ordered left-to-right, top-to-bottom.
///
/// ## Parameters
/// - `width`, `height`: Output image dimensions in pixels.
/// - `center_x`, `center_y`: Complex-plane coordinates at the center of the view.
///   Recommended default: `center_x = -0.5`, `center_y = 0.0`.
/// - `zoom`: Magnification factor (`1.0` shows the full Mandelbrot set; `2.0`
///   zooms in 2x, etc.). Non-positive or non-finite values default to `1.0`.
/// - `max_iter`: Maximum iterations per pixel (e.g., `200`–`500`). Higher values
///   reveal finer detail when zoomed in deeply, at the cost of more computation.
///
/// ## Coordinate Convention
/// - Increasing `center_x` pans **right**.
/// - Increasing `center_y` pans **up** (standard mathematical Y-axis pointing up).
#[wasm_bindgen]
pub fn render_mandelbrot(
    width: u32,
    height: u32,
    center_x: f64,
    center_y: f64,
    zoom: f64,
    max_iter: u32,
) -> Vec<u8> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    // Guard against invalid zoom values so we never divide by zero or NaN.
    let safe_zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        1.0
    };

    let w = width as usize;
    let h = height as usize;

    // Pre-allocate the exact buffer size (4 bytes per pixel: R, G, B, A)
    // so Rust does not need to reallocate while filling pixels.
    let mut pixels = vec![0_u8; w * h * 4];

    // Units in the complex plane per screen pixel.
    let min_dim = (width.min(height)) as f64;
    let scale = BASE_SPAN / (min_dim * safe_zoom);

    let half_w = (width as f64) * 0.5;
    let half_h = (height as f64) * 0.5;

    for py in 0..h {
        // Screen row `py = 0` is the top edge, so we subtract `(py - half_h)`
        // to make positive `center_y` point upward.
        let cy = center_y - ((py as f64) - half_h) * scale;
        let row_offset = py * w * 4;

        for px in 0..w {
            let cx = center_x + ((px as f64) - half_w) * scale;

            let escape = escape_time(cx, cy, max_iter);
            let rgba = smooth_color(escape);

            let idx = row_offset + px * 4;
            pixels[idx] = rgba[0];
            pixels[idx + 1] = rgba[1];
            pixels[idx + 2] = rgba[2];
            pixels[idx + 3] = rgba[3];
        }
    }

    pixels
}

// The `#[cfg(test)]` attribute tells Rust to compile and run this module only
// when executing `cargo test` — it is not included in the WebAssembly binary.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_and_bulb_points_are_inside_set() {
        // (0, 0) is inside the main cardioid: z stays 0 forever.
        assert_eq!(escape_time(0.0, 0.0, 200), None);
        // (-1, 0) is the center of the period-2 bulb: z alternates 0, -1, 0, -1...
        assert_eq!(escape_time(-1.0, 0.0, 200), None);
        // (-0.5, 0.2) is well inside the main cardioid.
        assert_eq!(escape_time(-0.5, 0.2, 200), None);
    }

    #[test]
    fn distant_points_escape_quickly() {
        // (2, 2) is far outside the Mandelbrot set and should escape in a few steps.
        let esc = escape_time(2.0, 2.0, 200).expect("(2, 2) must escape");
        assert!(esc < 5.0, "expected fast escape for (2, 2), got {esc}");

        // (1, 0) is outside the set: z goes 0 -> 1 -> 2 -> 5 -> 26...
        let esc_one = escape_time(1.0, 0.0, 200).expect("(1, 0) must escape");
        assert!(esc_one < 10.0, "expected fast escape for (1, 0), got {esc_one}");

        // (-2.5, 0) is left of the antenna tip at -2.0 and must escape.
        assert!(escape_time(-2.5, 0.0, 200).is_some());
    }

    #[test]
    fn boundary_takes_more_iterations_than_far_outside() {
        let far = escape_time(2.0, 2.0, 500).expect("(2, 2) should escape");
        let near_boundary =
            escape_time(-0.75, 0.01, 500).expect("(-0.75, 0.01) should escape");

        assert!(
            near_boundary > far,
            "expected near-boundary point ({near_boundary}) to take more iterations than far point ({far})"
        );
    }

    #[test]
    fn color_output_for_inside_and_outside_points() {
        // Inside the set -> pure opaque black.
        assert_eq!(smooth_color(None), [0, 0, 0, 255]);

        // Outside the set -> opaque and non-black.
        let outside = smooth_color(Some(12.5));
        assert_eq!(outside[3], 255, "alpha channel must always be 255");
        assert!(
            outside[0] > 0 || outside[1] > 0 || outside[2] > 0,
            "escaped point should not be pure black"
        );
    }

    #[test]
    fn smooth_color_is_continuous() {
        // Two very close smooth iteration values should produce very similar colors.
        let c1 = smooth_color(Some(20.0));
        let c2 = smooth_color(Some(20.05));
        for ch in 0..3 {
            let diff = (c1[ch] as i32 - c2[ch] as i32).abs();
            assert!(
                diff <= 5,
                "expected smooth transition on channel {ch}, got diff {diff} ({c1:?} vs {c2:?})"
            );
        }
    }

    #[test]
    fn render_buffer_dimensions_and_pixels() {
        let width = 32;
        let height = 24;
        let buf = render_mandelbrot(width, height, -0.5, 0.0, 1.0, 100);

        // Must be exactly width * height * 4 (RGBA) bytes.
        assert_eq!(buf.len(), (width * height * 4) as usize);

        // Every pixel's alpha channel should be 255 (fully opaque).
        for px in buf.chunks_exact(4) {
            assert_eq!(px[3], 255);
        }

        // The center pixel at (-0.5, 0.0) is inside the Mandelbrot set -> black.
        let center_idx = ((height / 2) * width + (width / 2)) as usize * 4;
        assert_eq!(&buf[center_idx..center_idx + 4], &[0, 0, 0, 255]);

        // The top-left corner pixel is far outside the set -> non-black color.
        assert_ne!(&buf[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn render_handles_zero_dimensions_and_invalid_zoom() {
        assert!(render_mandelbrot(0, 10, -0.5, 0.0, 1.0, 100).is_empty());
        assert!(render_mandelbrot(10, 0, -0.5, 0.0, 1.0, 100).is_empty());

        // Non-positive or NaN zoom should safely fall back to zoom = 1.0.
        let fallback_zero = render_mandelbrot(8, 8, -0.5, 0.0, 0.0, 50);
        let fallback_nan = render_mandelbrot(8, 8, -0.5, 0.0, f64::NAN, 50);
        let normal = render_mandelbrot(8, 8, -0.5, 0.0, 1.0, 50);
        assert_eq!(fallback_zero, normal);
        assert_eq!(fallback_nan, normal);
    }
}
