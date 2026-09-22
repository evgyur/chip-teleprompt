//! Fractional-position script rendering; the surrounding UI remains GDI.

use std::ptr::null_mut;
use windows::{
    core::{w, Error, Result, PCWSTR},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, RECT},
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::{
                FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY, HDC,
                LOGFONTW,
            },
        },
    },
};
use windows_numerics::{Matrix3x2, Vector2};
use windows_sys::Win32::{Foundation::RECT as SysRect, Graphics::Gdi as gdi};

struct TargetResources {
    brush: ID2D1SolidColorBrush,
    target: ID2D1DCRenderTarget,
}

pub struct TextRenderer {
    factory: ID2D1Factory,
    write_factory: IDWriteFactory,
    rendering_params: IDWriteRenderingParams,
    resources: Option<TargetResources>,
    layout: Option<IDWriteTextLayout>,
    _format: Option<IDWriteTextFormat>,
    color: D2D1_COLOR_F,
}

impl TextRenderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory =
                D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let write_factory = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)?;
            let defaults = write_factory.CreateRenderingParams()?;
            // Natural alone antialiases only horizontally. Symmetric mode also
            // interpolates vertical edges as the script moves between pixels.
            let rendering_params = write_factory.CreateCustomRenderingParams(
                defaults.GetGamma(),
                defaults.GetEnhancedContrast(),
                defaults.GetClearTypeLevel(),
                defaults.GetPixelGeometry(),
                DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
            )?;
            let mut renderer = Self {
                factory,
                write_factory,
                rendering_params,
                resources: None,
                layout: None,
                _format: None,
                color: color_from_gdi(0x00ffffff),
            };
            renderer.ensure_target()?;
            Ok(renderer)
        }
    }

    /// `text` excludes the trailing NUL; `width_dip` is the text width after
    /// subtracting both stage margins. Returns the actual formatted text height.
    pub fn update_layout(
        &mut self,
        text: &[u16],
        font: &gdi::LOGFONTW,
        size_pt: u32,
        width_dip: f32,
        color: u32,
    ) -> Result<f64> {
        // A failed update must not leave stale text available for a later draw.
        self.layout = None;
        self._format = None;
        if !width_dip.is_finite()
            || width_dip <= 0.0
            || size_pt == 0
            || text.len() > u32::MAX as usize
        {
            return Err(Error::new(E_INVALIDARG, "Invalid script layout dimensions"));
        }
        unsafe {
            let logfont = convert_logfont(font);
            let matched = self
                .write_factory
                .GetGdiInterop()?
                .CreateFontFromLOGFONT(&logfont)?;
            let family = matched.GetFontFamily()?;
            let collection = family.GetFontCollection()?;
            let names = family.GetFamilyNames()?;
            let mut name = vec![0u16; names.GetStringLength(0)? as usize + 1];
            names.GetString(0, &mut name)?;
            let format = self.write_factory.CreateTextFormat(
                PCWSTR(name.as_ptr()),
                &collection,
                matched.GetWeight(),
                matched.GetStyle(),
                matched.GetStretch(),
                size_pt as f32 * 96.0 / 72.0,
                w!("en-us"),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
            // The viewport clips rendering; its height must not truncate layout.
            let layout = self
                .write_factory
                .CreateTextLayout(text, &format, width_dip, f32::MAX)?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: text.len() as u32,
            };
            if !text.is_empty() {
                layout.SetUnderline(font.lfUnderline != 0, range)?;
                layout.SetStrikethrough(font.lfStrikeOut != 0, range)?;
            }
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            self.color = color_from_gdi(color);
            if let Some(resources) = &self.resources {
                resources.brush.SetColor(&self.color);
            }
            self._format = Some(format);
            self.layout = Some(layout);
            Ok(metrics.height.max(1.0) as f64)
        }
    }

    /// Draws into an existing backbuffer and restores all caller-owned DC state.
    ///
    /// # Safety
    /// `dc` must be a live writable DC, and the renderer must stay on its creating
    /// thread. The rectangle is expressed in physical DC pixels; `y` is in DIPs.
    pub unsafe fn draw(
        &mut self,
        dc: gdi::HDC,
        physical_stage: SysRect,
        dpi: u32,
        y: f64,
    ) -> Result<()> {
        if dc.is_null()
            || physical_stage.right <= physical_stage.left
            || physical_stage.bottom <= physical_stage.top
            || dpi == 0
            || !y.is_finite()
            || !(y as f32).is_finite()
        {
            return Err(Error::new(E_INVALIDARG, "Invalid script drawing surface"));
        }
        if self.layout.is_none() {
            return Err(Error::new(E_FAIL, "Script layout is unavailable"));
        }
        self.ensure_target()?;
        let saved = gdi::SaveDC(dc);
        if saved == 0 {
            return Err(Error::new(E_FAIL, "Cannot save script drawing context"));
        }
        let _restore = RestoreDc { dc, saved };
        // A DC target copies an already rasterized bitmap through GDI. Remove
        // the toolbar's anisotropic mapping to avoid scaling that bitmap twice.
        let identity = gdi::XFORM {
            eM11: 1.0,
            eM12: 0.0,
            eM21: 0.0,
            eM22: 1.0,
            eDx: 0.0,
            eDy: 0.0,
        };
        if gdi::SetGraphicsMode(dc, gdi::GM_ADVANCED) == 0
            || gdi::SetWorldTransform(dc, &identity) == 0
            || gdi::SetMapMode(dc, gdi::MM_TEXT) == 0
            || gdi::SetWindowOrgEx(dc, 0, 0, null_mut()) == 0
            || gdi::SetViewportOrgEx(dc, 0, 0, null_mut()) == 0
        {
            return Err(Error::new(E_FAIL, "Cannot reset script drawing transform"));
        }
        gdi::SetLayout(dc, gdi::LAYOUT_BITMAPORIENTATIONPRESERVED);
        let stage = RECT {
            left: physical_stage.left,
            top: physical_stage.top,
            right: physical_stage.right,
            bottom: physical_stage.bottom,
        };
        let result = (|| {
            let resources = self
                .resources
                .as_ref()
                .ok_or_else(|| Error::new(E_FAIL, "Script rendering target is unavailable"))?;
            let layout = self
                .layout
                .as_ref()
                .ok_or_else(|| Error::new(E_FAIL, "Script layout is unavailable"))?;
            let target = &resources.target;
            target.BindDC(HDC(dc), &stage)?;
            target.SetDpi(dpi as f32, dpi as f32);
            target.SetTransform(&Matrix3x2::identity());
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }));
            target.DrawTextLayout(
                Vector2 {
                    X: 16.0,
                    Y: y as f32,
                },
                layout,
                &resources.brush,
                D2D1_DRAW_TEXT_OPTIONS_NO_SNAP,
            );
            target.EndDraw(None, None)
        })();
        if result.is_err() {
            // Brushes belong to the target. Recreate both on the next draw;
            // DirectWrite layout and factories survive device loss.
            self.resources = None;
        }
        result
    }

    unsafe fn ensure_target(&mut self) -> Result<()> {
        if self.resources.is_some() {
            return Ok(());
        }
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target = self.factory.CreateDCRenderTarget(&properties)?;
        target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        target.SetTextRenderingParams(&self.rendering_params);
        let brush = target.CreateSolidColorBrush(&self.color, None)?;
        self.resources = Some(TargetResources { brush, target });
        Ok(())
    }
}

struct RestoreDc {
    dc: gdi::HDC,
    saved: i32,
}

impl Drop for RestoreDc {
    fn drop(&mut self) {
        unsafe {
            gdi::RestoreDC(self.dc, self.saved);
        }
    }
}

fn color_from_gdi(color: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: (color & 0xff) as f32 / 255.0,
        g: ((color >> 8) & 0xff) as f32 / 255.0,
        b: ((color >> 16) & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

fn convert_logfont(font: &gdi::LOGFONTW) -> LOGFONTW {
    LOGFONTW {
        lfHeight: font.lfHeight,
        lfWidth: font.lfWidth,
        lfEscapement: font.lfEscapement,
        lfOrientation: font.lfOrientation,
        lfWeight: font.lfWeight,
        lfItalic: font.lfItalic,
        lfUnderline: font.lfUnderline,
        lfStrikeOut: font.lfStrikeOut,
        lfCharSet: FONT_CHARSET(font.lfCharSet),
        lfOutPrecision: FONT_OUTPUT_PRECISION(font.lfOutPrecision),
        lfClipPrecision: FONT_CLIP_PRECISION(font.lfClipPrecision),
        lfQuality: FONT_QUALITY(font.lfQuality),
        lfPitchAndFamily: font.lfPitchAndFamily,
        lfFaceName: font.lfFaceName,
    }
}
