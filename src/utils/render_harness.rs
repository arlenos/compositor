//! Headless GPU render-readback test harness (Test Layer 1a).
//!
//! Builds a real [`GlesRenderer`] against an EGL device with no display server,
//! renders into an offscreen buffer, and reads the pixels back via
//! [`ExportMem`] - the Weston golden-image pattern. This lets a test assert the
//! compositor's render path actually produces the expected pixels without a
//! screen, so chrome rendering (titlebars, borders, window corners) can be
//! self-verified in CI / an agent shell instead of only on Tim's metal.
//!
//! The offscreen-render + readback mirrors [`crate::utils::screenshot`], which
//! already proves the fork's renderer implements `Offscreen<GlesRenderbuffer>`
//! + `ExportMem`; this module wraps the same calls behind a renderer that is
//! constructed headlessly (no DRM master, no winit window) so a test can drive
//! it directly.
//!
//! **Not bound by the sensing master switch, and this is the carve-out rather
//! than an exemption.** The switch binds a client asking for pixels; this builds
//! its own renderer against an EGL device with no display server, no session and
//! no client, so there is no screen for a capture to be of. Binding it would stop
//! CI from rendering a titlebar into a buffer, which protects nobody. The paths
//! that ARE bound, from the same search: the `ext-image-copy-capture-v1` handler
//! (`wayland::handlers::image_copy_capture`) and `utils::screenshot`.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{
            damage::OutputDamageTracker,
            element::RenderElement,
            gles::{GlesRenderbuffer, GlesRenderer},
            Bind, ExportMem, Offscreen,
        },
    },
    utils::{Rectangle, Size, Transform},
};

/// Construct a [`GlesRenderer`] on the first usable EGL device with no display
/// server. Tries each enumerated EGL device (a real render node via radeonsi,
/// or the Mesa software fallback) until one yields a working GL context.
///
/// Returns an error when no EGL device produces a renderer (e.g. a CI runner
/// with neither a render node nor a software GL stack); callers treat that as
/// "headless GPU unavailable here" and skip rather than fail.
pub fn headless_gles_renderer() -> Result<GlesRenderer> {
    let mut last_err: Option<anyhow::Error> = None;
    for device in EGLDevice::enumerate().context("enumerate EGL devices")? {
        match build_renderer(device) {
            Ok(renderer) => return Ok(renderer),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("no EGL devices available")))
}

fn build_renderer(device: EGLDevice) -> Result<GlesRenderer> {
    // SAFETY: each EGLDisplay is created once for this device and moved into the
    // context, which is moved into the renderer; nothing else binds the display.
    let display = unsafe { EGLDisplay::new(device) }.context("create EGL display")?;
    let context = EGLContext::new(&display).context("create EGL context")?;
    // SAFETY: the context is current on this thread for the renderer's lifetime;
    // the renderer owns it.
    let renderer = unsafe { GlesRenderer::new(context) }.context("create GlesRenderer")?;
    Ok(renderer)
}

/// Render `elements` over the `clear` colour into a `width` x `height` offscreen
/// buffer and read the result back as RGBA8 bytes (`width*height*4`, row-major,
/// no padding). `clear` is straight-alpha RGBA in `0.0..=1.0`. Pass an empty
/// slice to read back just the cleared buffer.
pub fn render_to_rgba<E>(
    renderer: &mut GlesRenderer,
    width: i32,
    height: i32,
    clear: [f32; 4],
    elements: &[E],
) -> Result<Vec<u8>>
where
    E: RenderElement<GlesRenderer>,
{
    let logical = Size::from((width, height));
    let format = Fourcc::Abgr8888;
    let mut buffer =
        Offscreen::<GlesRenderbuffer>::create_buffer(renderer, format, logical.to_buffer(1, Transform::Normal))
            .context("create offscreen buffer")?;
    let mut fb = renderer.bind(&mut buffer).context("bind offscreen buffer")?;

    let mut damage = OutputDamageTracker::new(logical.to_physical(1), 1.0, Transform::Normal);
    damage
        .render_output(renderer, &mut fb, 0, elements, clear)
        .map_err(|e| anyhow!("render_output failed: {e:?}"))?;

    let rect = Rectangle::new((0, 0).into(), logical);
    let mapping = renderer
        .copy_framebuffer(&fb, rect.to_buffer(1, Transform::Normal, &logical), format)
        .context("copy_framebuffer")?;
    let bytes = renderer.map_texture(&mapping).context("map_texture")?;
    Ok(bytes.to_vec())
}

/// Compare RGBA8 `pixels` against a golden PNG at `golden`, or record them as
/// the golden when the file is absent (the Weston first-run pattern). `tolerance`
/// is the max allowed per-channel delta (0 = exact). Returns an error describing
/// the first mismatch, or `Ok(())` on a match / a freshly-written golden.
pub fn assert_or_write_golden(
    pixels: &[u8],
    width: u32,
    height: u32,
    golden: &Path,
    tolerance: u8,
) -> Result<()> {
    if !golden.exists() {
        if let Some(parent) = golden.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let file = std::fs::File::create(golden).with_context(|| format!("create {golden:?}"))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .context("png header")?
            .write_image_data(pixels)
            .context("png data")?;
        return Ok(());
    }

    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(golden).with_context(|| format!("open {golden:?}"))?,
    ));
    let mut reader = decoder.read_info().context("png info")?;
    let buf_size = reader
        .output_buffer_size()
        .context("png output buffer size unknown")?;
    let mut golden_pixels = vec![0u8; buf_size];
    let info = reader.next_frame(&mut golden_pixels).context("png frame")?;
    let golden_pixels = &golden_pixels[..info.buffer_size()];

    if golden_pixels.len() != pixels.len() {
        return Err(anyhow!(
            "golden size {} != rendered size {} ({}x{})",
            golden_pixels.len(),
            pixels.len(),
            width,
            height
        ));
    }
    for (i, (g, p)) in golden_pixels.iter().zip(pixels).enumerate() {
        if g.abs_diff(*p) > tolerance {
            return Err(anyhow!(
                "pixel byte {i} differs: golden {g} vs rendered {p} (tolerance {tolerance})"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::backend::renderer::element::solid::SolidColorRenderElement;

    /// The harness self-test: render a known clear colour headlessly and read
    /// it back. Proves the EGL-device → offscreen-render → `ExportMem` readback
    /// path works without a display. Skips (does not fail) where no headless GL
    /// device exists, so a GPU-less CI runner stays green while a host with a
    /// render node (or Mesa software GL) actually verifies the path.
    #[test]
    fn headless_clear_reads_back_the_clear_colour() {
        let mut renderer = match headless_gles_renderer() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("render_harness: skipping, no headless GL device: {e:#}");
                return;
            }
        };

        // Opaque red, no elements: the whole buffer should read back red.
        let no_elements: Vec<SolidColorRenderElement> = Vec::new();
        let pixels = render_to_rgba(&mut renderer, 8, 8, [1.0, 0.0, 0.0, 1.0], &no_elements)
            .expect("render the clear colour");

        assert_eq!(pixels.len(), 8 * 8 * 4, "RGBA8, 8x8, no padding");
        // Abgr8888 fourcc => R,G,B,A byte order in memory; a 1.0/0/0/1.0 clear is
        // exact (no blending). Check every pixel.
        for (i, px) in pixels.chunks_exact(4).enumerate() {
            assert!(
                px[0] >= 250 && px[1] <= 5 && px[2] <= 5 && px[3] >= 250,
                "pixel {i} is not red: {px:?}"
            );
        }
    }
}
