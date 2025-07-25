// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use napi::bindgen_prelude::{
    AbortSignal, AsyncTask, Buffer, Either, Error as NapiError, Task, Undefined,
};
#[cfg(not(target_arch = "wasm32"))]
use napi_derive::napi;
use options::JsOptions;
use pathfinder_content::{
    outline::{Contour, Outline},
    stroke::{LineCap, LineJoin, OutlineStrokeToFill, StrokeStyle},
};
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;
use resvg::{
    tiny_skia::{PathSegment, Pixmap, Point},
    usvg::{self, ImageKind, NodeKind, TreeParsing, TreeTextToPath},
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{
    prelude::{wasm_bindgen, JsValue},
    JsCast,
};
use svgtypes::FontFamily;
use resvg::usvg::Node;

mod error;
mod fonts;
mod options;

use error::Error;
use usvg::NodeExt;

#[cfg(all(not(target_family = "wasm"), not(debug_assertions),))]
#[cfg(not(all(target_os = "linux", target_arch = "arm")))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Uint8Array | string")]
    pub type IStringOrBuffer;
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[cfg_attr(not(target_arch = "wasm32"), napi)]
#[derive(Debug)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[cfg_attr(not(target_arch = "wasm32"), napi)]
pub struct Resvg {
    tree: usvg::Tree,
    js_options: JsOptions,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[cfg_attr(not(target_arch = "wasm32"), napi)]
pub struct RenderedImage {
    pix: Pixmap,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[cfg_attr(not(target_arch = "wasm32"), napi)]
impl RenderedImage {
    // Wasm
    #[cfg(not(target_arch = "wasm32"))]
    #[napi]
    /// Write the image data to Buffer
    pub fn as_png(&self) -> Result<Buffer, NapiError> {
        let buffer = self.pix.encode_png().map_err(Error::from)?;
        Ok(buffer.into())
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(getter)]
    /// Get the PNG width
    pub fn width(&self) -> u32 {
        self.pix.width()
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(getter)]
    /// Get the PNG height
    pub fn height(&self) -> u32 {
        self.pix.height()
    }

    // napi-rs
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = asPng)]
    /// Write the image data to Uint8Array
    pub fn as_png(&self) -> Result<js_sys::Uint8Array, js_sys::Error> {
        let buffer = self.pix.encode_png().map_err(Error::from)?;
        Ok(buffer.as_slice().into())
    }

    /// Get the RGBA pixels of the image
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(getter)]
    pub fn pixels(&self) -> js_sys::Uint8Array {
        self.pix.data().into()
    }

    /// Get the RGBA pixels of the image
    #[cfg(not(target_arch = "wasm32"))]
    #[napi(getter)]
    pub fn pixels(&self) -> Buffer {
        self.pix.data().into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[napi(getter)]
    /// Get the PNG width
    pub fn width(&self) -> u32 {
        self.pix.width()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[napi(getter)]
    /// Get the PNG height
    pub fn height(&self) -> u32 {
        self.pix.height()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[napi]
impl Resvg {
    #[napi(constructor)]
    pub fn new(svg: Either<String, Buffer>, options: Option<String>) -> Result<Resvg, NapiError> {
        Resvg::new_inner(&svg, options)
    }

    fn new_inner(
        svg: &Either<String, Buffer>,
        options: Option<String>,
    ) -> Result<Resvg, NapiError> {
        let js_options: JsOptions = options
            .and_then(|o| serde_json::from_str(o.as_str()).ok())
            .unwrap_or_default();
        let _ = env_logger::builder()
            .filter_level(js_options.log_level)
            .try_init();

        let (mut opts, fontdb) = js_options.to_usvg_options();
        // Parse the SVG string into a tree.
        let mut tree = match svg {
            Either::A(a) => usvg::Tree::from_str(a.as_str(), &opts),
            Either::B(b) => usvg::Tree::from_data(b.as_ref(), &opts),
        }
        .map_err(|e| napi::Error::from_reason(format!("{e}")))?;

        // Кастомный обход для <text> без <tspan> с поддержкой textLayout
        if let Some(layout_map) = js_options.text_layout.as_ref() {
            // Первый проход: собираем нужные узлы
            let mut text_nodes = vec![];
            for node in tree.root.descendants() {
                if let usvg::NodeKind::Text(ref text) = *node.borrow() {
                    // Пропускаем, если есть <tspan>
                    if node.has_children() {
                        continue;
                    }
                    let id = text.id.clone();
                    if layout_map.0.get(&id).is_some() {
                        text_nodes.push(node.clone());
                    }
                }
            }
            // --- Сбор параметров для вставки новых <text> ---
            struct TextInsert {
                after_id: String,
                lines: Vec<String>,
                x: f32,
                y: f32,
                line_height: f32,
                font_family: String,
                font_size: f32,
                id: String,
                orig_chunks: Vec<usvg::TextChunk>, // добавляем исходные чанки
            }
            let mut inserts = Vec::new();
            for node in &text_nodes {
                let node_ref = node.clone();
                let (after_id, lines, x, y, line_height, font_family, font_size, id, orig_chunks) = {
                    let mut borrow = node_ref.borrow_mut();
                    if let usvg::NodeKind::Text(ref mut text) = *borrow {
                        println!("[DEBUG] text.chunks: {:?}", text.chunks.iter().map(|c| c.text.clone()).collect::<Vec<_>>());
                        let text_content = text.chunks.iter().map(|c| c.text.replace("\\n", "\n")).collect::<String>();
                        println!("[DEBUG] text_content (после замены \\n): {:?}", text_content);
                        let id = text.id.clone();
                        let opts = layout_map.0.get(&id).unwrap();
                        let letter_spacing = opts.letter_spacing.unwrap_or(0.0);
                        let font_family = opts.font_family.as_ref().map(|s| s.as_str()).unwrap_or(&js_options.font.default_font_family);
                        let font_size = opts.font_size.unwrap_or(js_options.font.default_font_size);
                        let max_width = opts.width.unwrap_or(300.0);
                        let maxlines = opts.maxlines.map(|v| v as usize);
                        let line_height = opts.line_height.unwrap_or(font_size * 1.2);
                        let x = opts.x.unwrap_or_else(|| text.chunks.first().and_then(|c| c.x).unwrap_or(0.0));
                        let y = opts.y.unwrap_or_else(|| text.chunks.first().and_then(|c| c.y).unwrap_or(0.0));
                        let lines = crate::fonts::wrap_text_greedy(
                            &text_content,
                            font_family,
                            font_size,
                            &fontdb,
                            max_width,
                            maxlines,
                            letter_spacing,
                        );
                        let orig_chunks = text.chunks.clone();
                        text.chunks.clear();
                        (id.clone(), lines, x, y, line_height, font_family.to_string(), font_size, id, orig_chunks)
                    } else {
                        continue;
                    }
                };
                inserts.push(TextInsert {
                    after_id,
                    lines,
                    x,
                    y,
                    line_height,
                    font_family,
                    font_size,
                    id,
                    orig_chunks,
                });
            }
            // --- Вставка новых <text> после обхода дерева ---
            for ins in inserts {
                // Ищем узел по id
                let mut after_node = None;
                for node in tree.root.descendants() {
                    if let usvg::NodeKind::Text(ref text) = *node.borrow() {
                        if text.id == ins.after_id {
                            after_node = Some(node.clone());
                            break;
                        }
                    }
                }
                if let Some(mut last) = after_node {
                    for (i, line) in ins.lines.iter().enumerate() {
                        let opts = js_options.text_layout.as_ref().and_then(|map| map.0.get(&ins.id));
                        // --- fill ---
                        let fill_color = opts
                            .and_then(|o| o.fill.as_ref())
                            .and_then(|s| {
                                let s = s.trim_start_matches('#');
                                if s.len() == 6 {
                                    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                                    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                                    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                                    Some(usvg::Color::new_rgb(r, g, b))
                                } else {
                                    None
                                }
                            })
                            .or_else(|| ins.orig_chunks.first().and_then(|c| c.spans.first()).and_then(|span| span.fill.as_ref()).and_then(|fill| match &fill.paint {
                                usvg::Paint::Color(color) => Some(*color),
                                _ => None,
                            }))
                            .unwrap_or(usvg::Color::new_rgb(0, 0, 0));
                        // --- opacity ---
                        let opacity = opts
                            .and_then(|o| o.opacity)
                            .or_else(|| ins.orig_chunks.first().and_then(|c| c.spans.first()).and_then(|span| span.fill.as_ref()).map(|fill| fill.opacity.get()))
                            .unwrap_or(1.0);
                        // --- letter-spacing ---
                        let letter_spacing = opts
                            .and_then(|o| o.letter_spacing)
                            .or_else(|| ins.orig_chunks.first().and_then(|c| c.spans.first()).map(|span| span.letter_spacing))
                            .unwrap_or(0.0);
                        // --- центрирование и прочее ---
                        let text_align = opts.and_then(|o| o.text_align.as_ref().map(|s| s.as_str())).unwrap_or("left");
                        let container_width = opts.and_then(|o| o.width).unwrap_or(300.0);
                        let line_width = crate::fonts::measure_text_width(
                            line,
                            &ins.font_family,
                            ins.font_size,
                            &fontdb,
                            letter_spacing,
                        ).unwrap_or(0.0);
                        let mut x = ins.x;
                        match text_align {
                            "center" | "middle" => {
                                x = ins.x + (container_width - line_width) / 2.0;
                            }
                            "right" => {
                                x = ins.x + (container_width - line_width);
                            }
                            _ => {}
                        }
                        let y_pos = ins.y + i as f32 * ins.line_height;
                        let mut new_text = usvg::Text {
                            id: String::new(),
                            transform: usvg::Transform::default(),
                            rendering_mode: usvg::TextRendering::OptimizeLegibility,
                            positions: vec![],
                            rotate: vec![],
                            writing_mode: usvg::WritingMode::LeftToRight,
                            chunks: vec![],
                        };
                        new_text.chunks.push(usvg::TextChunk {
                            text: line.clone(),
                            x: Some(x),
                            y: Some(y_pos),
                            anchor: usvg::TextAnchor::Start,
                            spans: vec![{
                                let span = usvg::TextSpan {
                                    start: 0,
                                    end: line.as_bytes().len(),
                                    font: usvg::Font {
                                        families: vec![ins.font_family.clone()],
                                        style: usvg::FontStyle::Normal,
                                        stretch: usvg::FontStretch::Normal,
                                        weight: 400,
                                    },
                                    font_size: usvg::NonZeroPositiveF32::new(ins.font_size).unwrap_or(usvg::NonZeroPositiveF32::new(12.0).unwrap()),
                                    fill: Some(usvg::Fill {
                                        paint: usvg::Paint::Color(fill_color),
                                        opacity: usvg::NormalizedF32::new(opacity).unwrap_or(usvg::NormalizedF32::new(1.0).unwrap()),
                                        rule: usvg::FillRule::NonZero,
                                    }),
                                    alignment_baseline: usvg::AlignmentBaseline::Auto,
                                    apply_kerning: true,
                                    baseline_shift: vec![],
                                    decoration: usvg::TextDecoration {
                                        underline: None,
                                        overline: None,
                                        line_through: None,
                                    },
                                    dominant_baseline: usvg::DominantBaseline::Auto,
                                    letter_spacing: letter_spacing,
                                    paint_order: usvg::PaintOrder::default(),
                                    stroke: None,
                                    text_length: None,
                                    length_adjust: usvg::LengthAdjust::Spacing,
                                    visibility: usvg::Visibility::Visible,
                                    word_spacing: 0.0,
                                    small_caps: false,
                                };
                                println!("[DEBUG] <text id={}>: span: start={}, end={}, font_family={:?}, font_size={}, fill={:?}, opacity={}, letter_spacing={}", ins.id, span.start, span.end, span.font.families, span.font_size.get(), fill_color, opacity, letter_spacing);
                                span
                            }],
                            text_flow: usvg::TextFlow::Linear,
                        });
                        let new_node = Node::new(NodeKind::Text(new_text));
                        last.insert_after(new_node.clone());
                        last = new_node;
                        println!("[DEBUG] <text id={}>: new line '{}', x={:?}, y={:?}, align={}", ins.id, line, x, y_pos, text_align);
                    }
                }
            }
        }
        tree.convert_text(&fontdb);
        Ok(Resvg { tree, js_options })
    }

    #[napi]
    /// Renders an SVG in Node.js
    pub fn render(&self) -> Result<RenderedImage, NapiError> {
        Ok(self.render_inner()?)
    }

    #[napi]
    /// Output usvg-simplified SVG string
    pub fn to_string(&self) -> String {
        use usvg::TreeWriting;
        self.tree.to_string(&usvg::XmlOptions::default())
    }

    #[napi(js_name = innerBBox)]
    /// Calculate a maximum bounding box of all visible elements in this SVG.
    ///
    /// Note: path bounding box are approx values.

    // Either<T, Undefined> depends on napi 2.4.3
    // https://github.com/napi-rs/napi-rs/releases/tag/napi@2.4.3
    pub fn inner_bbox(&self) -> Either<BBox, Undefined> {
        let rect = self.tree.view_box.rect;
        let rect = points_to_rect(
            Vector2F::new(rect.x(), rect.y()),
            Vector2F::new(rect.right(), rect.bottom()),
        );
        let mut v = None;
        for child in self.tree.root.children() {
            let child_viewbox = match self.node_bbox(child).and_then(|v| v.intersection(rect)) {
                Some(v) => v,
                None => continue,
            };
            if let Some(v) = v.as_mut() {
                *v = child_viewbox.union_rect(*v);
            } else {
                v = Some(child_viewbox)
            };
        }
        match v {
            Some(v) => Either::A(BBox {
                x: v.min_x().floor() as f64,
                y: v.min_y().floor() as f64,
                width: (v.max_x().ceil() - v.min_x().floor()) as f64,
                height: (v.max_y().ceil() - v.min_y().floor()) as f64,
            }),
            None => Either::B(()),
        }
    }

    #[napi(js_name = getBBox)]
    /// Calculate a maximum bounding box of all visible elements in this SVG.
    /// This will first apply transform.
    /// Similar to `SVGGraphicsElement.getBBox()` DOM API.

    // Either<T, Undefined> depends on napi 2.4.3
    // https://github.com/napi-rs/napi-rs/releases/tag/napi@2.4.3
    pub fn get_bbox(&self) -> Either<BBox, Undefined> {
        match self.tree.root.calculate_bbox() {
            Some(bbox) => Either::A(BBox {
                x: bbox.x() as f64,
                y: bbox.y() as f64,
                width: bbox.width() as f64,
                height: bbox.height() as f64,
            }),
            None => Either::B(()),
        }
    }

    #[napi(js_name = cropByBBox)]
    /// Use a given `BBox` to crop the svg. Currently this method simply changes
    /// the viewbox/size of the svg and do not move the elements for simplicity
    pub fn crop_by_bbox(&mut self, bbox: &BBox) {
        if !bbox.width.is_finite() || !bbox.height.is_finite() {
            return;
        }
        let width = bbox.width as f32;
        let height = bbox.height as f32;
        self.tree.view_box.rect =
            usvg::NonZeroRect::from_xywh(bbox.x as f32, bbox.y as f32, width, height).unwrap();
        self.tree.size = usvg::Size::from_wh(width, height).unwrap();
    }

    #[napi]
    pub fn images_to_resolve(&self) -> Result<Vec<String>, NapiError> {
        Ok(self.images_to_resolve_inner()?)
    }

    #[napi]
    pub fn resolve_image(&self, href: String, buffer: Buffer) -> Result<(), NapiError> {
        let buffer = buffer.to_vec();
        Ok(self.resolve_image_inner(href, buffer)?)
    }

    /// Get the SVG width
    #[napi(getter)]
    pub fn width(&self) -> f32 {
        self.tree.size.width().round()
    }

    /// Get the SVG height
    #[napi(getter)]
    pub fn height(&self) -> f32 {
        self.tree.size.height().round()
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Resvg {
    #[wasm_bindgen(constructor)]
    pub fn new(
        svg: IStringOrBuffer,
        options: Option<String>,
        custom_font_buffers: Option<js_sys::Array>,
    ) -> Result<Resvg, js_sys::Error> {
        let js_options: JsOptions = options
            .and_then(|o| serde_json::from_str(o.as_str()).ok())
            .unwrap_or_default();

        let (mut opts, mut fontdb) = js_options.to_usvg_options();

        crate::fonts::load_wasm_fonts(&js_options.font, custom_font_buffers, &mut fontdb)?;

        let mut tree = if js_sys::Uint8Array::instanceof(&svg) {
            let uintarray = js_sys::Uint8Array::unchecked_from_js_ref(&svg);
            let svg_buffer = uintarray.to_vec();
            usvg::Tree::from_data(&svg_buffer, &opts).map_err(Error::from)
        } else if let Some(s) = svg.as_string() {
            usvg::Tree::from_str(s.as_str(), &opts).map_err(Error::from)
        } else {
            Err(Error::InvalidInput)
        }?;
        tree.convert_text(&fontdb);
        Ok(Resvg { tree, js_options })
    }

    /// Get the SVG width
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> f32 {
        self.tree.size.width().round()
    }

    /// Get the SVG height
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> f32 {
        self.tree.size.height().round()
    }

    /// Renders an SVG in Wasm
    pub fn render(&self) -> Result<RenderedImage, js_sys::Error> {
        Ok(self.render_inner()?)
    }

    /// Output usvg-simplified SVG string
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        use usvg::TreeWriting;
        self.tree.to_string(&usvg::XmlOptions::default())
    }

    /// Calculate a maximum bounding box of all visible elements in this SVG.
    ///
    /// Note: path bounding box are approx values.
    #[wasm_bindgen(js_name = innerBBox)]
    pub fn inner_bbox(&self) -> Option<BBox> {
        let rect = self.tree.view_box.rect;
        let rect = points_to_rect(
            Vector2F::new(rect.x(), rect.y()),
            Vector2F::new(rect.right(), rect.bottom()),
        );
        let mut v = None;
        for child in self.tree.root.children() {
            let child_viewbox = match self.node_bbox(child).and_then(|v| v.intersection(rect)) {
                Some(v) => v,
                None => continue,
            };
            if let Some(v) = v.as_mut() {
                *v = child_viewbox.union_rect(*v);
            } else {
                v = Some(child_viewbox)
            };
        }
        let v = v?;
        Some(BBox {
            x: v.min_x().floor() as f64,
            y: v.min_y().floor() as f64,
            width: (v.max_x().ceil() - v.min_x().floor()) as f64,
            height: (v.max_y().ceil() - v.min_y().floor()) as f64,
        })
    }

    #[wasm_bindgen(js_name = getBBox)]
    /// Calculate a maximum bounding box of all visible elements in this SVG.
    /// This will first apply transform.
    /// Similar to `SVGGraphicsElement.getBBox()` DOM API.
    pub fn get_bbox(&self) -> Option<BBox> {
        let bbox = self.tree.root.calculate_bbox()?;
        Some(BBox {
            x: bbox.x() as f64,
            y: bbox.y() as f64,
            width: bbox.width() as f64,
            height: bbox.height() as f64,
        })
    }

    #[wasm_bindgen(js_name = cropByBBox)]
    /// Use a given `BBox` to crop the svg. Currently this method simply changes
    /// the viewbox/size of the svg and do not move the elements for simplicity
    pub fn crop_by_bbox(&mut self, bbox: &BBox) {
        if !bbox.width.is_finite() || !bbox.height.is_finite() {
            return;
        }
        let width = bbox.width as f32;
        let height = bbox.height as f32;
        self.tree.view_box.rect =
            usvg::NonZeroRect::from_xywh(bbox.x as f32, bbox.y as f32, width, height).unwrap();
        self.tree.size = usvg::Size::from_wh(width, height).unwrap();
    }

    #[wasm_bindgen(js_name = imagesToResolve)]
    pub fn images_to_resolve(&self) -> Result<js_sys::Array, js_sys::Error> {
        let images = self.images_to_resolve_inner()?;
        let result = js_sys::Array::from_iter(images.into_iter().map(|s| JsValue::from(s)));
        Ok(result)
    }

    #[wasm_bindgen(js_name = resolveImage)]
    pub fn resolve_image(
        &self,
        href: String,
        buffer: js_sys::Uint8Array,
    ) -> Result<(), js_sys::Error> {
        let buffer = buffer.to_vec();
        Ok(self.resolve_image_inner(href, buffer)?)
    }
}

impl Resvg {
    fn node_bbox(&self, node: usvg::Node) -> Option<RectF> {
        let transform = node.borrow().transform();
        let bbox = match &*node.borrow() {
            usvg::NodeKind::Path(p) => {
                let no_fill = p.fill.is_none()
                    || p.fill
                        .as_ref()
                        .map(|f| f.opacity.get() == 0.0)
                        .unwrap_or_default();
                let no_stroke = p.stroke.is_none()
                    || p.stroke
                        .as_ref()
                        .map(|f| f.opacity.get() == 0.0)
                        .unwrap_or_default();
                if no_fill && no_stroke {
                    return None;
                }
                let mut outline = Outline::new();
                let mut contour = Contour::new();
                let mut iter = p.data.segments().peekable();
                while let Some(seg) = iter.next() {
                    match seg {
                        PathSegment::MoveTo(p) => {
                            if !contour.is_empty() {
                                outline
                                    .push_contour(std::mem::replace(&mut contour, Contour::new()));
                            }
                            contour.push_endpoint(Vector2F::new(p.x, p.y))
                        }
                        PathSegment::LineTo(p) => {
                            let v = Vector2F::new(p.x, p.y);
                            if let Some(PathSegment::Close) = iter.peek() {
                                let first = contour.position_of(0);
                                if (first - v).square_length() < 1.0 {
                                    continue;
                                }
                            }
                            contour.push_endpoint(v);
                        }
                        PathSegment::CubicTo(p1, p2, p) => {
                            contour.push_cubic(
                                Vector2F::new(p1.x, p1.y),
                                Vector2F::new(p2.x, p2.y),
                                Vector2F::new(p.x, p.y),
                            );
                        }
                        PathSegment::QuadTo(p1, p) => {
                            contour
                                .push_quadratic(Vector2F::new(p1.x, p1.y), Vector2F::new(p.x, p.y));
                        }
                        PathSegment::Close => {
                            contour.close();
                            outline.push_contour(std::mem::replace(&mut contour, Contour::new()));
                        }
                    }
                }
                if !contour.is_empty() {
                    outline.push_contour(std::mem::replace(&mut contour, Contour::new()));
                }
                if let Some(stroke) = p.stroke.as_ref() {
                    if !no_stroke {
                        let mut style = StrokeStyle::default();
                        style.line_width = stroke.width.get() as f32;
                        style.line_join = LineJoin::Miter(style.line_width);
                        style.line_cap = match stroke.linecap {
                            usvg::LineCap::Butt => LineCap::Butt,
                            usvg::LineCap::Round => LineCap::Round,
                            usvg::LineCap::Square => LineCap::Square,
                        };
                        let mut filler = OutlineStrokeToFill::new(&outline, style);
                        filler.offset();
                        outline = filler.into_outline();
                    }
                }
                Some(outline.bounds())
            }
            usvg::NodeKind::Group(g) => {
                let clippath = if let Some(clippath) =
                    g.clip_path.as_ref().and_then(|n| n.root.first_child())
                {
                    self.node_bbox(clippath)
                } else if let Some(mask) = g.mask.as_ref().and_then(|n| n.root.first_child()) {
                    self.node_bbox(mask)
                } else {
                    Some(self.viewbox())
                }?;
                let mut v = None;
                for child in node.children() {
                    let child_viewbox =
                        match self.node_bbox(child).and_then(|v| v.intersection(clippath)) {
                            Some(v) => v,
                            None => continue,
                        };
                    if let Some(v) = v.as_mut() {
                        *v = child_viewbox.union_rect(*v);
                    } else {
                        v = Some(child_viewbox)
                    };
                }
                v.and_then(|v| v.intersection(self.viewbox()))
            }
            usvg::NodeKind::Image(image) => {
                let rect = image.view_box.rect;
                Some(points_to_rect(
                    Vector2F::new(rect.x(), rect.y()),
                    Vector2F::new(rect.right(), rect.bottom()),
                ))
            }
            usvg::NodeKind::Text(_) => None,
        }?;
        let mut pts = vec![
            Point::from_xy(bbox.min_x(), bbox.min_y()),
            Point::from_xy(bbox.max_x(), bbox.max_y()),
            Point::from_xy(bbox.min_x(), bbox.max_y()),
            Point::from_xy(bbox.max_x(), bbox.min_y()),
        ];
        transform.map_points(&mut pts);
        let x_min = pts[0].x.min(pts[1].x).min(pts[2].x).min(pts[3].x);
        let x_max = pts[0].x.max(pts[1].x).max(pts[2].x).max(pts[3].x);
        let y_min = pts[0].y.min(pts[1].y).min(pts[2].y).min(pts[3].y);
        let y_max = pts[0].y.max(pts[1].y).max(pts[2].y).max(pts[3].y);
        let r = points_to_rect(Vector2F::new(x_min, y_min), Vector2F::new(x_max, y_max));
        Some(r)
    }

    fn viewbox(&self) -> RectF {
        RectF::new(
            Vector2F::new(0.0, 0.0),
            Vector2F::new(self.width() as f32, self.height() as f32),
        )
    }

    fn render_inner(&self) -> Result<RenderedImage, Error> {
        let (width, height, transform) = self.js_options.fit_to.fit_to(self.tree.size)?;
        let mut pixmap = self.js_options.create_pixmap(width, height)?;
        // Render the tree
        let _image = resvg::Tree::from_usvg(&self.tree).render(transform, &mut pixmap.as_mut());

        // Crop the SVG
        let crop_rect = resvg::tiny_skia::IntRect::from_ltrb(
            self.js_options.crop.left,
            self.js_options.crop.top,
            self.js_options.crop.right.unwrap_or(width as i32),
            self.js_options.crop.bottom.unwrap_or(height as i32),
        );

        if let Some(crop_rect) = crop_rect {
            pixmap = pixmap.clone_rect(crop_rect).unwrap_or(pixmap);
        }

        Ok(RenderedImage { pix: pixmap })
    }

    fn images_to_resolve_inner(&self) -> Result<Vec<String>, Error> {
        let mut data = vec![];
        for node in self.tree.root.descendants() {
            if let NodeKind::Image(i) = &mut *node.borrow_mut() {
                if let ImageKind::RAW(_, _, buffer) = &mut i.kind {
                    let s = String::from_utf8(buffer.as_slice().to_vec())?;
                    data.push(s);
                }
            }
        }
        Ok(data)
    }

    fn resolve_image_inner(&self, href: String, buffer: Vec<u8>) -> Result<(), Error> {
        let resolver = usvg::ImageHrefResolver::default_data_resolver();
        let (options, _) = self.js_options.to_usvg_options();
        let mime = MimeType::parse(&buffer)?.mime_type().to_string();

        for node in self.tree.root.descendants() {
            if let NodeKind::Image(i) = &mut *node.borrow_mut() {
                let matched = if let ImageKind::RAW(_, _, data) = &mut i.kind {
                    let s = String::from_utf8(data.as_slice().to_vec()).map_err(Error::from)?;
                    s == href
                } else {
                    false
                };
                if matched {
                    let data = (resolver)(&mime, Arc::new(buffer.clone()), &options);
                    if let Some(kind) = data {
                        i.kind = kind;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct AsyncRenderer {
    options: Option<String>,
    svg: Either<String, Buffer>,
}

#[cfg(not(target_arch = "wasm32"))]
#[napi]
impl Task for AsyncRenderer {
    type Output = RenderedImage;
    type JsValue = RenderedImage;

    fn compute(&mut self) -> Result<Self::Output, NapiError> {
        let resvg = Resvg::new_inner(&self.svg, self.options.clone())?;
        resvg.render()
    }

    fn resolve(
        &mut self,
        _env: napi::Env,
        result: Self::Output,
    ) -> Result<Self::JsValue, NapiError> {
        Ok(result)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[napi]
pub fn render_async(
    svg: Either<String, Buffer>,
    options: Option<String>,
    signal: Option<AbortSignal>,
) -> AsyncTask<AsyncRenderer> {
    match signal {
        Some(s) => AsyncTask::with_signal(AsyncRenderer { options, svg }, s),
        None => AsyncTask::new(AsyncRenderer { options, svg }),
    }
}

fn points_to_rect(min: Vector2F, max: Vector2F) -> RectF {
    RectF::new(min, max - min)
}

// Detects the file type by magic number.
// Currently resvg only supports the following types of files.
pub enum MimeType {
    Png,
    Jpeg,
    Gif,
}

impl MimeType {
    pub fn parse(buffer: &[u8]) -> Result<Self, Error> {
        if buffer.len() < 4 {
            return Err(Error::UnsupportedImage);
        }
        Ok(match &buffer[0..4] {
            [0x89, 0x50, 0x4E, 0x47] => MimeType::Png,
            [0xFF, 0xD8, 0xFF, _] => MimeType::Jpeg,
            [0x47, 0x49, 0x46, _] => MimeType::Gif,
            _ => return Err(Error::UnsupportedImage),
        })
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            MimeType::Png => "image/png",
            MimeType::Gif => "image/gif",
            _ => "image/jpeg",
        }
    }
}
