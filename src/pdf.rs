use pdf_writer::types::{
    ActionType, AnnotationType, CidFontType, FontFlags, SystemInfo, UnicodeCmap,
};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::css::{BackgroundRepeat, BackgroundSize, TextAlign};
use crate::layout::{Document, FontId, InlineItem, Page};

struct FontRefs {
    type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    to_unicode: Ref,
}

pub fn write_pdf(document: &Document) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog_id = Ref::new(1);
    let pages_id = Ref::new(2);
    let font_ids = [Ref::new(3), Ref::new(4), Ref::new(5), Ref::new(6)];
    let image_refs = (0..document.images.len())
        .map(|index| Ref::new(7 + index as i32))
        .collect::<Vec<_>>();
    let font_base_id = 7 + image_refs.len() as i32;
    let font_refs = (0..document.fonts.len())
        .map(|index| {
            let base = font_base_id + index as i32 * 5;
            FontRefs {
                type0: Ref::new(base),
                cid: Ref::new(base + 1),
                descriptor: Ref::new(base + 2),
                file: Ref::new(base + 3),
                to_unicode: Ref::new(base + 4),
            }
        })
        .collect::<Vec<_>>();
    let mut next_id = font_base_id + document.fonts.len() as i32 * 5;
    let info_id = Ref::new(next_id);
    next_id += 1;

    let page_ids = (0..document.pages.len())
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let content_ids = (0..document.pages.len())
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let destinations_id = Ref::new(next_id);
    next_id += 1;
    let opacities = collect_opacities(document);
    let state_ids = (0..opacities.len())
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let destinations = document
        .anchors
        .iter()
        .filter_map(|anchor| {
            let geometry = document.pages.get(anchor.page)?.geometry;
            Some((
                anchor.name.clone(),
                anchor.page,
                geometry.height - anchor.top,
            ))
        })
        .collect::<Vec<_>>();

    {
        let mut dictionary = pdf.destinations(destinations_id);
        for (name, page_index, top) in &destinations {
            dictionary
                .insert(Name(name.as_bytes()))
                .page(page_ids[*page_index])
                .xyz(0.0, *top, Some(0.0));
        }
    }
    {
        let mut catalog = pdf.catalog(catalog_id);
        catalog.pages(pages_id);
        if !destinations.is_empty() {
            catalog.destinations(destinations_id);
        }
    }

    let mut info = pdf.document_info(info_id);
    info.creator(TextStr("html2pdf-rs"))
        .producer(TextStr("html2pdf-rs"));
    if let Some(title) = &document.metadata.title {
        info.title(TextStr(title));
    }
    if let Some(author) = &document.metadata.author {
        info.author(TextStr(author));
    }
    if let Some(subject) = &document.metadata.subject {
        info.subject(TextStr(subject));
    }
    if let Some(keywords) = &document.metadata.keywords {
        info.keywords(TextStr(keywords));
    }
    info.finish();

    for (&id, base) in font_ids.iter().zip([
        b"Helvetica".as_slice(),
        b"Helvetica-Bold".as_slice(),
        b"Helvetica-Oblique".as_slice(),
        b"Helvetica-BoldOblique".as_slice(),
    ]) {
        pdf.type1_font(id)
            .base_font(Name(base))
            .encoding_predefined(Name(b"WinAnsiEncoding"));
    }

    for (&id, &alpha) in state_ids.iter().zip(opacities.iter()) {
        pdf.ext_graphics(id)
            .non_stroking_alpha(alpha)
            .stroking_alpha(alpha);
    }

    for (asset, &id) in document.images.iter().zip(image_refs.iter()) {
        pdf.image_xobject(id, asset.pixels.as_slice())
            .width(asset.width as i32)
            .height(asset.height as i32)
            .color_space_name(Name(b"DeviceRGB"))
            .bits_per_component(8)
            .finish();
    }

    for (font, refs) in document.fonts.iter().zip(font_refs.iter()) {
        write_embedded_font(&mut pdf, font, refs);
    }

    for (page_index, page) in document.pages.iter().enumerate() {
        let page_id = page_ids[page_index];
        let content_id = content_ids[page_index];

        let content = build_content(page, document, page_index, document.pages.len(), &opacities);
        pdf.stream(content_id, content.as_slice()).finish();

        let mut links = Vec::new();
        for line in &page.lines {
            for link in &line.links {
                let link_id = Ref::new(next_id);
                next_id += 1;
                let target = link.uri.strip_prefix('#').and_then(|fragment| {
                    document
                        .anchors
                        .iter()
                        .find(|anchor| anchor.name == fragment)
                        .and_then(|anchor| {
                            page_ids.get(anchor.page).map(|&target_page_id| {
                                (
                                    target_page_id,
                                    document.pages[anchor.page].geometry.height - anchor.top,
                                )
                            })
                        })
                });
                write_link(
                    &mut pdf,
                    link_id,
                    page_id,
                    &page.geometry,
                    line,
                    link,
                    target,
                );
                links.push(link_id);
            }
        }

        let mut page_writer = pdf.page(page_id);
        page_writer.parent(pages_id).media_box(Rect::new(
            0.0,
            0.0,
            page.geometry.width,
            page.geometry.height,
        ));

        {
            let mut resources = page_writer.resources();
            let mut fonts = resources.fonts();
            fonts.pair(Name(b"F1"), font_ids[0]);
            fonts.pair(Name(b"F2"), font_ids[1]);
            fonts.pair(Name(b"F3"), font_ids[2]);
            fonts.pair(Name(b"F4"), font_ids[3]);
            for (index, _) in document.fonts.iter().enumerate() {
                let name = format!("E{}", index + 1);
                fonts.pair(Name(name.as_bytes()), font_refs[index].type0);
            }
            fonts.finish();

            if !state_ids.is_empty() {
                let mut states = resources.ext_g_states();
                for (index, &id) in state_ids.iter().enumerate() {
                    let name = format!("GS{}", index + 1);
                    states.pair(Name(name.as_bytes()), id);
                }
            }

            let mut x_objects = resources.x_objects();
            for (index, &id) in image_refs.iter().enumerate() {
                let name = format!("Im{}", index + 1);
                x_objects.pair(Name(name.as_bytes()), id);
            }
        }

        page_writer.contents(content_id);
        if !links.is_empty() {
            page_writer.annotations(links);
        }
        page_writer.finish();
    }

    pdf.pages(pages_id)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);

    pdf.finish()
}

fn write_embedded_font(pdf: &mut Pdf, font: &crate::layout::EmbeddedFont, refs: &FontRefs) {
    let info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };

    pdf.stream(refs.file, font.data.as_slice())
        .pair(Name(b"Length1"), font.data.len() as i32)
        .finish();

    let mut cmap = UnicodeCmap::<u16>::new(Name(b"Custom"), info);
    for (&ch, &glyph) in &font.glyphs {
        cmap.pair(glyph, ch);
    }
    pdf.stream(refs.to_unicode, cmap.finish().as_slice())
        .finish();

    pdf.font_descriptor(refs.descriptor)
        .name(Name(font.family.as_bytes()))
        .flags(FontFlags::NON_SYMBOLIC)
        .bbox(Rect::new(
            0.0,
            font.descent * 1000.0,
            1000.0,
            font.ascent * 1000.0,
        ))
        .italic_angle(0.0)
        .ascent(font.ascent * 1000.0)
        .descent(font.descent * 1000.0)
        .cap_height(font.ascent * 700.0)
        .stem_v(50.0)
        .font_file2(refs.file)
        .finish();

    let mut cid = pdf.cid_font(refs.cid);
    cid.subtype(CidFontType::Type2)
        .base_font(Name(font.family.as_bytes()))
        .system_info(info)
        .font_descriptor(refs.descriptor)
        .cid_to_gid_map_predefined(Name(b"Identity"));
    {
        let mut widths = cid.widths();
        for (&ch, &glyph) in &font.glyphs {
            let width = font.advance.get(&ch).copied().unwrap_or(0.5) * 1000.0;
            widths.consecutive(glyph, [width]);
        }
        widths.finish();
    }
    cid.finish();

    pdf.type0_font(refs.type0)
        .base_font(Name(font.family.as_bytes()))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(refs.cid)
        .to_unicode(refs.to_unicode)
        .finish();
}

fn write_link(
    pdf: &mut Pdf,
    link_id: Ref,
    page_id: Ref,
    geometry: &crate::layout::PageGeometry,
    line: &crate::layout::Line,
    link: &crate::layout::LinkRect,
    target: Option<(Ref, f32)>,
) {
    let x = geometry.margin + line.left + link.x;
    let top = geometry.height - geometry.margin - line.top;
    let bottom = top - line.height;

    let mut annotation = pdf.annotation(link_id);
    annotation
        .subtype(AnnotationType::Link)
        .rect(Rect::new(x, bottom, x + link.width, top))
        .page(page_id);
    {
        let mut action = annotation.action();
        if let Some((target_page_id, target_top)) = target {
            action
                .action_type(ActionType::GoTo)
                .destination()
                .page(target_page_id)
                .xyz(0.0, target_top, Some(0.0));
        } else {
            action
                .action_type(ActionType::Uri)
                .uri(Str(link.uri.as_bytes()));
        }
    }
    annotation.finish();
}

fn font_resource(font: FontId) -> Vec<u8> {
    match font {
        FontId::Builtin(kind) => kind.resource_name().to_vec(),
        FontId::Embedded(index) => format!("E{}", index + 1).into_bytes(),
    }
}

fn to_cids(text: &str, font: &crate::layout::EmbeddedFont) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.chars().count() * 2);
    for ch in text.chars() {
        let glyph = font.glyphs.get(&ch).copied().unwrap_or(0);
        bytes.extend_from_slice(&glyph.to_be_bytes());
    }
    bytes
}

fn collect_opacities(document: &Document) -> Vec<f32> {
    let mut values: Vec<f32> = Vec::new();
    for page in &document.pages {
        for line in &page.lines {
            let alpha = line.opacity.clamp(0.0, 1.0);
            if alpha < 0.999 && !values.iter().any(|value| (*value - alpha).abs() < 0.001) {
                values.push(alpha);
            }
        }
    }
    values
}

fn rounded_rect_path(content: &mut Content, x: f32, y: f32, width: f32, height: f32, radius: f32) {
    let radius = radius.min(width * 0.5).min(height * 0.5).max(0.0);
    if radius == 0.0 {
        content.rect(x, y, width, height);
        return;
    }

    let k = radius * 0.552_284_8;
    content
        .move_to(x + radius, y)
        .line_to(x + width - radius, y)
        .cubic_to(
            x + width - radius + k,
            y,
            x + width,
            y + radius - k,
            x + width,
            y + radius,
        )
        .line_to(x + width, y + height - radius)
        .cubic_to(
            x + width,
            y + height - radius + k,
            x + width - radius + k,
            y + height,
            x + width - radius,
            y + height,
        )
        .line_to(x + radius, y + height)
        .cubic_to(
            x + radius - k,
            y + height,
            x,
            y + height - radius + k,
            x,
            y + height - radius,
        )
        .line_to(x, y + radius)
        .cubic_to(x, y + radius - k, x + radius - k, y, x + radius, y)
        .close_path();
}

fn build_content(
    page: &Page,
    document: &Document,
    page_index: usize,
    page_count: usize,
    opacities: &[f32],
) -> Vec<u8> {
    let mut content = Content::new();
    let width = page.geometry.width;
    let height = page.geometry.height;
    let margin = page.geometry.margin;

    let mut drawable = page
        .lines
        .iter()
        .map(|line| (0, line))
        .chain(
            page.overlays
                .iter()
                .map(|overlay| (overlay.z, &overlay.line)),
        )
        .collect::<Vec<_>>();
    drawable.sort_by_key(|(z, _)| *z);

    for (_, line) in drawable {
        let alpha_state = if line.opacity < 0.999 {
            opacities
                .iter()
                .position(|value| (value - line.opacity).abs() < 0.001)
        } else {
            None
        };
        if let Some(state) = alpha_state {
            content.save_state();
            let name = format!("GS{}", state + 1);
            content.op("gs").operand(Name(name.as_bytes()));
        }

        let x = match line.align {
            TextAlign::Left | TextAlign::Justify => margin,
            TextAlign::Center => margin + (width - 2.0 * margin - line.width) / 2.0,
            TextAlign::Right => margin + width - 2.0 * margin - line.width,
        } + line.left;
        let line_y = height - margin - line.top - line.height;
        let line_width = line.background_width.unwrap_or(line.width);

        if let Some(index) = line.background_image
            && let Some(asset) = document.images.get(index)
        {
            let natural_width = asset.width as f32 * document.px_scale;
            let natural_height = asset.height as f32 * document.px_scale;
            let (tile_width, tile_height) = match line.background_size {
                BackgroundSize::Auto => (natural_width, natural_height),
                BackgroundSize::Cover => {
                    let scale = line_width / natural_width.max(1.0);
                    let cover = line.height / natural_height.max(1.0);
                    let scale = scale.max(cover);
                    (natural_width * scale, natural_height * scale)
                }
                BackgroundSize::Contain => {
                    let scale = line_width / natural_width.max(1.0);
                    let contain = line.height / natural_height.max(1.0);
                    let scale = scale.min(contain);
                    (natural_width * scale, natural_height * scale)
                }
            };
            if tile_width > 0.0 && tile_height > 0.0 {
                content.save_state();
                rounded_rect_path(
                    &mut content,
                    margin + line.left,
                    line_y,
                    line_width,
                    line.height,
                    line.border_radius,
                );
                content.clip_nonzero();
                let name = format!("Im{}", index + 1);
                let mut y = line_y;
                while y < line_y + line.height {
                    let mut x_tile = margin + line.left;
                    while x_tile < margin + line.left + line_width {
                        content
                            .transform([
                                tile_width,
                                0.0,
                                0.0,
                                tile_height,
                                x_tile,
                                y + line.height - tile_height,
                            ])
                            .x_object(Name(name.as_bytes()));
                        if line.background_repeat == BackgroundRepeat::NoRepeat {
                            break;
                        }
                        x_tile += tile_width;
                    }
                    if line.background_repeat == BackgroundRepeat::NoRepeat {
                        break;
                    }
                    y += tile_height;
                }
                content.restore_state();
            }
        }

        if let Some(color) = line.background {
            content.set_fill_rgb(color[0], color[1], color[2]);
            rounded_rect_path(
                &mut content,
                margin + line.left,
                line_y,
                line_width,
                line.height,
                line.border_radius,
            );
            content.fill_nonzero();
        }

        if let Some(border) = line.border {
            let color = border.color;
            let width = border.width;
            content
                .set_fill_rgb(color[0], color[1], color[2])
                .set_line_width(width);

            let rounded = line.border_radius > 0.0
                && border.top
                && border.bottom
                && border.left
                && border.right;
            if rounded {
                content.set_stroke_rgb(color[0], color[1], color[2]);
                rounded_rect_path(
                    &mut content,
                    margin + line.left + width * 0.5,
                    line_y + width * 0.5,
                    line_width - width,
                    line.height - width,
                    line.border_radius,
                );
                content.stroke();
            }

            if border.top && !rounded {
                content.rect(
                    margin + line.left,
                    line_y + line.height - width,
                    line_width,
                    width,
                );
            }
            if border.bottom && !rounded {
                content.rect(margin + line.left, line_y, line_width, width);
            }
            if border.left && !rounded {
                content.rect(margin + line.left, line_y, width, line.height);
            }
            if border.right && !rounded {
                content.rect(
                    margin + line.left + line_width - width,
                    line_y,
                    width,
                    line.height,
                );
            }
            content.fill_nonzero();
        }

        let max_size = line
            .items
            .iter()
            .filter_map(|item| match item {
                InlineItem::Text(run) => Some(run.size),
                InlineItem::Image(_) => None,
            })
            .fold(0.0f32, f32::max);
        let baseline = line_y + line.height - max_size * 0.18;
        let mut cursor = x;
        let extra_per_space = if line.align == TextAlign::Justify {
            let gaps = line
                .items
                .iter()
                .filter(|item| matches!(item, InlineItem::Text(run) if run.text == " "))
                .count();
            if gaps == 0 {
                0.0
            } else {
                let available = width - 2.0 * margin - line.left;
                (available - line.width) / gaps as f32
            }
        } else {
            0.0
        };

        for item in &line.items {
            match item {
                InlineItem::Text(run) => {
                    let name = font_resource(run.font);
                    let bytes = match run.font {
                        FontId::Builtin(_) => to_win_ansi(&run.text),
                        FontId::Embedded(index) => to_cids(&run.text, &document.fonts[index]),
                    };

                    if bytes.is_empty() {
                        continue;
                    }

                    let spacing = run.spacing
                        + if run.text == " " {
                            extra_per_space
                        } else {
                            0.0
                        };
                    let width = run.width
                        + if run.text == " " {
                            extra_per_space
                        } else {
                            0.0
                        };

                    content
                        .begin_text()
                        .set_char_spacing(spacing)
                        .set_text_matrix([1.0, 0.0, 0.0, 1.0, cursor, baseline])
                        .set_font(Name(name.as_slice()), run.size)
                        .set_fill_rgb(run.color[0], run.color[1], run.color[2])
                        .show(Str(bytes.as_slice()))
                        .set_char_spacing(0.0)
                        .end_text();

                    let decorations = [
                        (run.underline, baseline - run.size * 0.12),
                        (run.line_through, baseline - run.size * 0.32),
                        (run.overline, baseline + run.size * 0.78),
                    ];
                    if decorations.iter().any(|(enabled, _)| *enabled) {
                        content
                            .set_stroke_rgb(run.color[0], run.color[1], run.color[2])
                            .set_line_width(0.6);
                        for (enabled, y) in decorations {
                            if enabled {
                                content
                                    .move_to(cursor, y)
                                    .line_to(cursor + width, y)
                                    .stroke();
                            }
                        }
                    }

                    cursor += width;
                }
                InlineItem::Image(image) => {
                    let top = line.top + (line.height - image.height) / 2.0;
                    let y = height - margin - top - image.height;
                    let name = format!("Im{}", image.image + 1);

                    content
                        .save_state()
                        .transform([image.width, 0.0, 0.0, image.height, cursor, y])
                        .x_object(Name(name.as_bytes()))
                        .restore_state();

                    cursor += image.width;
                }
            }
        }

        if alpha_state.is_some() {
            content.restore_state();
        }
    }

    if let Some(template) = &document.page_text {
        let text = template
            .replace("{PAGE_NUM}", &(page_index + 1).to_string())
            .replace("{PAGE_COUNT}", &page_count.to_string());
        let bytes = to_win_ansi(&text);
        if !bytes.is_empty() {
            let text_width = bytes.len() as f32 * 4.5;
            let (x, y) = match document.page_text_location {
                crate::PageTextLocation::TopLeft => (margin, height - margin * 0.5),
                crate::PageTextLocation::TopCenter => {
                    ((width - text_width) / 2.0, height - margin * 0.5)
                }
                crate::PageTextLocation::TopRight => {
                    (width - margin - text_width, height - margin * 0.5)
                }
                crate::PageTextLocation::BottomLeft => (margin, margin * 0.35),
                crate::PageTextLocation::BottomCenter => {
                    ((width - text_width) / 2.0, margin * 0.35)
                }
                crate::PageTextLocation::BottomRight => {
                    (width - margin - text_width, margin * 0.35)
                }
            };
            content
                .begin_text()
                .set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y])
                .set_font(Name(b"F1"), 9.0)
                .set_fill_rgb(0.4, 0.4, 0.4)
                .show(Str(bytes.as_slice()))
                .end_text();
        }
    }

    content.finish().into_vec()
}

fn to_win_ansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|ch| match ch as u32 {
            32..=126 => ch as u8,
            160..=255 => ch as u8,
            _ => b'?',
        })
        .collect()
}
