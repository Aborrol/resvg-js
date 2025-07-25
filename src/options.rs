// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::Arc;
use std::collections::HashMap;

use crate::error::Error;
#[cfg(not(target_arch = "wasm32"))]
use napi::{bindgen_prelude::Buffer, Either};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::fontdb::Database;
use resvg::usvg::{self, ImageHrefResolver, ImageKind, Options, TreeParsing};
use serde::{Deserialize, Deserializer};

/// Image fit options.
/// This provides the deserializer for `usvg::FitTo`.
#[derive(Deserialize)]
#[serde(
    tag = "mode",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum FitToDef {
    /// Keep original size.
    Original,
    /// Scale to width.
    Width(u32),
    /// Scale to height.
    Height(u32),
    /// Zoom by factor.
    Zoom(f32),
}

impl FitToDef {
    pub(crate) fn fit_to(&self, size: usvg::Size) -> Result<(u32, u32, Transform), Error> {
        let mut transform = Transform::identity();
        let width = size.width();
        let height = size.height();
        let scale = match self {
            FitToDef::Original => 1.0,
            FitToDef::Width(w) => *w as f32 / width,
            FitToDef::Height(h) => *h as f32 / height,
            FitToDef::Zoom(s) => *s,
        };
        let width = (width * scale).round().max(0.0) as u32;
        let height = (height * scale).round().max(0.0) as u32;
        transform = transform.pre_scale(
            width as f32 / size.width() as f32,
            height as f32 / size.height() as f32,
        );
        if width == 0 || height == 0 {
            Err(Error::ZeroSized)
        } else {
            Ok((width, height, transform))
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase", remote = "log::LevelFilter")]
enum LogLevelDef {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

pub(crate) trait ResvgReadable {
    fn load(&self, options: &usvg::Options) -> Result<usvg::Tree, usvg::Error>;
}

impl<'a> ResvgReadable for &'a str {
    fn load(&self, options: &usvg::Options) -> Result<usvg::Tree, usvg::Error> {
        usvg::Tree::from_str(self, options)
    }
}

impl<'a> ResvgReadable for &'a [u8] {
    fn load(&self, options: &usvg::Options) -> Result<usvg::Tree, usvg::Error> {
        usvg::Tree::from_data(self, options)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> ResvgReadable for &'a Either<String, Buffer> {
    fn load(&self, options: &usvg::Options) -> Result<usvg::Tree, usvg::Error> {
        match self {
            Either::A(s) => s.as_str().load(options),
            Either::B(b) => b.as_ref().load(options),
        }
    }
}

/// The javascript options passed to `render()`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct JsOptions {
    /// Font related options.
    pub font: JsFontOptions,

    /// Target DPI.
    ///
    /// Impact units conversion.
    ///
    /// Note: This is not the DPI in the PNG file. Resvg does not change the DPI
    ///  of the PNG file.
    /// https://github.com/RazrFalcon/resvg/issues/451#issuecomment-914462093
    /// https://github.com/RazrFalcon/resvg/issues/526#issuecomment-1190433890
    ///
    /// Default: 96.0
    pub dpi: f32,

    /// A list of languages.
    ///
    /// Will be used to resolve a `systemLanguage` conditional attribute.
    ///
    /// Format: en, en-US.
    ///
    /// Default: [en]
    pub languages: Vec<String>,

    /// The size to render the SVG.
    ///
    /// Default: Original
    pub fit_to: FitToDef,

    /// The background color of the SVG.
    ///
    /// Default: `None`
    pub background: Option<String>,

    /// Crop options
    pub crop: JsCropOptions,

    #[serde(with = "LogLevelDef")]
    pub log_level: log::LevelFilter,

    #[serde(default)]
    pub text_layout: Option<JsTextLayoutOptions>,
}

impl Default for JsOptions {
    fn default() -> JsOptions {
        JsOptions {
            font: JsFontOptions::default(),
            dpi: 96.0,
            languages: vec!["en".to_string()],
            fit_to: FitToDef::Original,
            background: None,
            crop: JsCropOptions::default(),
            log_level: log::LevelFilter::Error,
            text_layout: None,
        }
    }
}

impl JsOptions {
    pub(crate) fn to_usvg_options(&self) -> (usvg::Options, Database) {
        // Load fonts
        #[cfg(not(target_arch = "wasm32"))]
        let fontdb = crate::fonts::load_fonts(&self.font);
        #[cfg(target_arch = "wasm32")]
        let fontdb = Database::new();

        // Build the SVG options
        let opts = usvg::Options {
            resources_dir: None,
            dpi: self.dpi,
            font_family: self.font.default_font_family.clone(),
            font_size: self.font.default_font_size,
            languages: self.languages.clone(),
            shape_rendering: usvg::ShapeRendering::default(),
            text_rendering: usvg::TextRendering::default(),
            image_rendering: usvg::ImageRendering::default(),
            default_size: usvg::Size::from_wh(100.0, 100.0).unwrap(),
            image_href_resolver: usvg::ImageHrefResolver::default(),
        };
        (opts, fontdb)
    }

    pub(crate) fn create_pixmap(&self, width: u32, height: u32) -> Result<Pixmap, Error> {
        // Parse the background
        let background = self
            .background
            .as_ref()
            .map(|color| color.parse::<svgtypes::Color>())
            .transpose()?;

        // Unwrap is safe, because `size` is already valid.
        let mut pixmap = Pixmap::new(width, height).unwrap();

        if let Some(bg) = background {
            let color = resvg::tiny_skia::Color::from_rgba8(bg.red, bg.green, bg.blue, bg.alpha);
            pixmap.fill(color);
        }
        Ok(pixmap)
    }
}

/// The font options passed to `load_fonts()`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsFontBuffer {
    pub font_name: String,
    #[serde(with = "serde_bytes")]
    pub buffer: Vec<u8>,
}

/// The font options passed to `load_fonts()`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct JsFontOptions {
    /// If system fonts should be loaded.
    ///
    /// Default: true
    pub load_system_fonts: bool,

    /// A list of local font file paths to load.
    pub font_files: Vec<String>,

    /// A list of local font directories to load.
    pub font_dirs: Vec<String>,

    /// The default font family.
    ///
    /// Will be used when no `font-family` attribute is set in the SVG.
    ///
    /// Default: ""
    pub default_font_family: String,

    /// The default font size.
    ///
    /// Will be used when no `font-size` attribute is set in the SVG.
    ///
    /// Default: 12
    pub default_font_size: f32,

    /// The 'serif' font family.
    ///
    /// Default: Times New Roman
    pub serif_family: String,

    /// The 'sans-serif' font family.
    ///
    /// Default: Arial
    pub sans_serif_family: String,

    /// The 'cursive' font family.
    ///
    /// Default: Comic Sans MS
    pub cursive_family: String,

    /// The 'fantasy' font family.
    ///
    /// Default: Impact
    pub fantasy_family: String,

    /// The 'monospace' font family.
    ///
    /// Default: Courier New
    pub monospace_family: String,
    #[serde(default)]
    pub font_buffers: Vec<JsFontBuffer>,
}

impl Default for JsFontOptions {
    fn default() -> JsFontOptions {
        JsFontOptions {
            load_system_fonts: true,
            font_files: vec![],
            font_dirs: vec![],
            default_font_family: "".to_string(),
            default_font_size: 12.0,
            serif_family: "Times New Roman".to_string(),
            sans_serif_family: "Arial".to_string(),
            cursive_family: "Comic Sans MS".to_string(),
            fantasy_family: "Impact".to_string(),
            monospace_family: "Courier New".to_string(),
            font_buffers: vec![],
        }
    }
}

/// The font options passed to `load_fonts()`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct JsCropOptions {
    /// The rectangle's left x-axis coordinate.
    ///
    /// Default: 0
    pub left: i32,

    /// The rectangle's top y-axis coordinate.
    ///
    /// Default: 0
    pub top: i32,

    /// The rectangle's right x-axis coordinate. `None` targets the svg width.
    ///
    /// Default: None
    pub right: Option<i32>,

    /// The rectangle's bottom y-axis coordinate. `None` targets the svg height.
    ///
    /// Default: None
    pub bottom: Option<i32>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JsTextLayoutBlockOptions {
    pub width: Option<f32>,
    pub line_height: Option<f32>,
    pub maxlines: Option<u32>,
    pub text_align: Option<String>, // например, "center", "left", "right"
    // SVG-атрибуты, которые можно перезаписать
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub fill: Option<String>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub opacity: Option<f32>,
    pub letter_spacing: Option<f32>,
    // Можно добавить любые другие SVG-атрибуты по мере необходимости
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JsTextLayoutOptions(pub HashMap<String, JsTextLayoutBlockOptions>);