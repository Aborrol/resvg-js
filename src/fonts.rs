// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::options::JsFontOptions;
use resvg::usvg::fontdb::{Database, Language};

#[cfg(not(target_arch = "wasm32"))]
use log::{debug, warn};

#[cfg(not(target_arch = "wasm32"))]
use resvg::usvg::fontdb::{Family, Query, Source};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
use woff2::decode::{convert_woff2_to_ttf, is_woff2};

/// Loads fonts.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_fonts(font_options: &JsFontOptions) -> Database {
    // Create a new font database
    let mut fontdb = Database::new();
    let now = std::time::Instant::now();

    // Загружаем буферы шрифтов с явным именем
    let mut loaded_from_buffer = 0;
    for font in &font_options.font_buffers {
        fontdb.load_font_data(font.buffer.clone());
        loaded_from_buffer += 1;
    }
    println!("[DEBUG] Загружено шрифтов из буфера: {}", loaded_from_buffer);
    // Debug: выводим все загруженные family name после загрузки буферов
    for face in fontdb.faces() {
        println!("[DEBUG] После буферов: {:?}", face.families);
    }

    // 加载指定路径的字体
    let mut loaded_from_files = 0;
    for path in &font_options.font_files {
        if let Err(e) = fontdb.load_font_file(path) {
            warn!("Failed to load '{}' cause {}.", path, e);
        } else {
            loaded_from_files += 1;
        }
    }
    println!("[DEBUG] Загружено шрифтов из файлов: {}", loaded_from_files);

    // Load font directories
    let mut loaded_from_dirs = 0;
    for path in &font_options.font_dirs {
        fontdb.load_fonts_dir(path);
        loaded_from_dirs += 1;
    }
    println!("[DEBUG] Загружено директорий шрифтов: {}", loaded_from_dirs);

    // 加载系统字体
    if font_options.load_system_fonts {
        fontdb.load_system_fonts();
        println!("[DEBUG] Загружены системные шрифты");
    }

    // Итоговый список всех шрифтов
    println!("[DEBUG] Итоговый список всех шрифтов в fontdb:");
    for face in fontdb.faces() {
        println!("[DEBUG] {:?}", face.families);
    }

    set_font_families(font_options, &mut fontdb);

    debug!(
        "Loaded {} font faces in {}ms.",
        fontdb.len(),
        now.elapsed().as_micros() as f64 / 1000.0
    );

    fontdb
}

/// Loads fonts in Wasm.
#[cfg(target_arch = "wasm32")]
pub fn load_wasm_fonts(
    font_options: &JsFontOptions,
    font_buffers: Option<js_sys::Array>,
    fontdb: &mut Database,
) -> Result<(), js_sys::Error> {
    if let Some(ref font_buffers) = font_buffers {
        for font in font_buffers.values().into_iter() {
            let raw_font = font?;
            let font_data = raw_font.dyn_into::<js_sys::Uint8Array>()?.to_vec();

            let font_buffer = if is_woff2(&font_data) {
                convert_woff2_to_ttf(&mut std::io::Cursor::new(font_data)).unwrap()
            } else {
                font_data
            };
            fontdb.load_font_data(font_buffer);
        }
    }

    set_wasm_font_families(font_options, fontdb, font_buffers);

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn set_font_families(font_options: &JsFontOptions, fontdb: &mut Database) {
    let mut default_font_family = font_options.default_font_family.clone().trim().to_string();
    // Debug: get font lists
    // for face in fontdb.faces() {
    //     let family = face
    //         .families
    //         .iter()
    //         .find(|f| f.1 == Language::English_UnitedStates)
    //         .unwrap_or(&face.families[0]);
    //     debug!("font_id = {}, family_name = {}", face.id, family.0);
    // }

    let fontdb_found_default_font_family = fontdb
        .faces()
        .find_map(|it| {
            it.families
                .iter()
                .find(|f| f.0 == default_font_family)
                .map(|f| f.0.clone())
        })
        .unwrap_or_default();

    // 当 default_font_family 为空或系统无该字体时，尝试把 fontdb
    // 中字体列表的第一个字体设置为默认的字体。
    if default_font_family.is_empty() || fontdb_found_default_font_family.is_empty() {
        // font_files 或 font_dirs 选项不为空时, 从已加载的字体列表中获取第一个字体的 font family。
        if !font_options.font_files.is_empty() || !font_options.font_dirs.is_empty() {
            default_font_family = get_first_font_family_or_fallback(fontdb);
        }
    }

    fontdb.set_serif_family(&default_font_family);
    fontdb.set_sans_serif_family(&default_font_family);
    fontdb.set_cursive_family(&default_font_family);
    fontdb.set_fantasy_family(&default_font_family);
    fontdb.set_monospace_family(&default_font_family);

    debug!("📝 default_font_family = '{}'", default_font_family);

    #[cfg(not(target_arch = "wasm32"))]
    find_and_debug_font_path(fontdb, default_font_family.as_str())
}

#[cfg(target_arch = "wasm32")]
fn set_wasm_font_families(
    font_options: &JsFontOptions,
    fontdb: &mut Database,
    font_buffers: Option<js_sys::Array>,
) {
    let mut default_font_family = font_options.default_font_family.clone().trim().to_string();

    let fontdb_found_default_font_family = fontdb
        .faces()
        .find_map(|it| {
            it.families
                .iter()
                .find(|f| f.0 == default_font_family)
                .map(|f| f.0.clone())
        })
        .unwrap_or_default();

    // 当 default_font_family 为空或系统无该字体时，尝试把 fontdb
    // 中字体列表的第一个字体设置为默认的字体。
    if default_font_family.is_empty() || fontdb_found_default_font_family.is_empty() {
        // font_buffers 选项不为空时, 从已加载的字体列表中获取第一个字体的 font family。
        if let Some(_font_buffers) = font_buffers {
            default_font_family = get_first_font_family_or_fallback(fontdb);
        }
    }

    fontdb.set_serif_family(&default_font_family);
    fontdb.set_sans_serif_family(&default_font_family);
    fontdb.set_cursive_family(&default_font_family);
    fontdb.set_fantasy_family(&default_font_family);
    fontdb.set_monospace_family(&default_font_family);
}

/// 查询指定 font family 的字体是否存在，如果不存在则使用 fallback_font_family 代替。
#[cfg(not(target_arch = "wasm32"))]
fn find_and_debug_font_path(fontdb: &mut Database, font_family: &str) {
    let query = Query {
        families: &[Family::Name(font_family)],
        ..Query::default()
    };

    let now = std::time::Instant::now();
    // 查询当前使用的字体是否存在
    match fontdb.query(&query) {
        Some(id) => {
            let (src, index) = fontdb.face_source(id).unwrap();
            if let Source::File(ref path) = &src {
                debug!(
                    "Font '{}':{} found in {}ms.",
                    path.display(),
                    index,
                    now.elapsed().as_micros() as f64 / 1000.0
                );
            }
        }
        None => {
            let first_font_family = get_first_font_family_or_fallback(fontdb);

            fontdb.set_serif_family(&first_font_family);
            fontdb.set_sans_serif_family(&first_font_family);
            fontdb.set_cursive_family(&first_font_family);
            fontdb.set_fantasy_family(&first_font_family);
            fontdb.set_monospace_family(&first_font_family);

            warn!(
                "Warning: The default font-family '{}' not found, set to '{}'.",
                font_family, first_font_family,
            );
        }
    }
}

/// 获取 fontdb 中的第一个字体的 font family。
fn get_first_font_family_or_fallback(fontdb: &mut Database) -> String {
    let mut default_font_family = "Arial".to_string(); // 其他情况都 fallback 到指定的这个字体。

    match fontdb.faces().next() {
        Some(face) => {
            let base_family = face
                .families
                .iter()
                .find(|f| f.1 == Language::English_UnitedStates)
                .unwrap_or(&face.families[0]);

            default_font_family = base_family.0.clone();
        }
        None => {
            #[cfg(not(target_arch = "wasm32"))]
            debug!(
                "📝 get_first_font_family not found = '{}'",
                default_font_family
            );
        }
    }

    default_font_family
}

#[cfg(not(target_arch = "wasm32"))]
pub fn measure_text_width(
    text: &str,
    font_family: &str,
    font_size: f32,
    fontdb: &resvg::usvg::fontdb::Database,
    letter_spacing: f32,
) -> Option<f32> {
    use resvg::usvg::fontdb::{Family, Query, Source};
    use fontdue::Font;

    // 1. Найти нужный шрифт по имени
    let query = Query {
        families: &[Family::Name(font_family)],
        ..Query::default()
    };
    let face_id = fontdb.query(&query)?;
    let face = fontdb.face(face_id)?;

    // 2. Извлечь TTF-данные
    let ttf_bytes: Vec<u8> = match face.source {
        Source::Binary(ref bytes) => bytes.as_ref().as_ref().to_vec(),
        Source::File(ref path) => {
            match std::fs::read(path) {
                Ok(data) => data,
                Err(_) => return None,
            }
        }
        _ => return None,
    };

    // 3. Создать fontdue::Font
    let font = match Font::from_bytes(ttf_bytes.as_slice(), fontdue::FontSettings::default()) {
        Ok(f) => f,
        Err(_) => return None,
    };

    // 4. Измерить ширину текста (без рендеринга)
    let mut width = 0.0f32;
    let mut char_count = 0;
    for ch in text.chars() {
        let (metrics, _) = font.rasterize(ch, font_size);
        width += metrics.advance_width;
        char_count += 1;
    }
    if char_count > 1 {
        width += (char_count - 1) as f32 * letter_spacing;
    }
    Some(width)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn wrap_text_greedy(
    text: &str,
    font_family: &str,
    font_size: f32,
    fontdb: &resvg::usvg::fontdb::Database,
    max_width: f32,
    maxlines: Option<usize>,
    letter_spacing: f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current_line = String::new();
        for word in paragraph.split_whitespace() {
            let test_line = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };
            if let Some(width) = measure_text_width(&test_line, font_family, font_size, fontdb, letter_spacing) {
                if width <= max_width {
                    current_line = test_line;
                } else {
                    if !current_line.is_empty() {
                        lines.push(std::mem::take(&mut current_line));
                    }
                    // Если слово само длиннее max_width — переносим по символам
                    if let Some(word_width) = measure_text_width(word, font_family, font_size, fontdb, letter_spacing) {
                        if word_width > max_width {
                            let mut part = String::new();
                            for ch in word.chars() {
                                let test_part = format!("{}{}", part, ch);
                                if let Some(part_width) = measure_text_width(&test_part, font_family, font_size, fontdb, letter_spacing) {
                                    if part_width <= max_width {
                                        part = test_part;
                                    } else {
                                        lines.push(part);
                                        part = ch.to_string();
                                    }
                                }
                            }
                            current_line = part;
                        } else {
                            current_line = word.to_string();
                        }
                    }
                }
            }
        }
        if !current_line.is_empty() {
            lines.push(std::mem::take(&mut current_line));
        }
    }
    // Ограничиваем по maxlines, добавляем ... если нужно
    if let Some(max) = maxlines {
        if lines.len() > max {
            let mut limited = lines.into_iter().take(max).collect::<Vec<_>>();
            if let Some(last) = limited.last_mut() {
                last.push_str("...");
            }
            return limited;
        }
    }
    lines
}
