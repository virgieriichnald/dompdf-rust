use pdf_writer::types::{
    ActionType, AnnotationType, CidFontType, FontFlags, SystemInfo, UnicodeCmap,
};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::css::TextAlign;
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

    pdf.catalog(catalog_id).pages(pages_id);

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

    let mut page_ids = Vec::with_capacity(document.pages.len());

    for page in &document.pages {
        let page_id = Ref::new(next_id);
        next_id += 1;
        let content_id = Ref::new(next_id);
        next_id += 1;
        page_ids.push(page_id);

        let content = build_content(page, document);
        pdf.stream(content_id, content.as_slice()).finish();

        let mut links = Vec::new();
        for line in &page.lines {
            for link in &line.links {
                let link_id = Ref::new(next_id);
                next_id += 1;
                write_link(&mut pdf, link_id, page_id, document, line, link);
                links.push(link_id);
            }
        }

        let mut page_writer = pdf.page(page_id);
        page_writer.parent(pages_id).media_box(Rect::new(
            0.0,
            0.0,
            document.width,
            document.height,
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
    document: &Document,
    line: &crate::layout::Line,
    link: &crate::layout::LinkRect,
) {
    let x = document.margin + line.left + link.x;
    let top = document.height - document.margin - line.top;
    let bottom = top - line.height;

    let mut annotation = pdf.annotation(link_id);
    annotation
        .subtype(AnnotationType::Link)
        .rect(Rect::new(x, bottom, x + link.width, top))
        .page(page_id);
    annotation
        .action()
        .action_type(ActionType::Uri)
        .uri(Str(link.uri.as_bytes()));
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

fn build_content(page: &Page, document: &Document) -> Vec<u8> {
    let mut content = Content::new();

    for line in &page.lines {
        let x = match line.align {
            TextAlign::Left | TextAlign::Justify => document.margin,
            TextAlign::Center => {
                document.margin + (document.width - 2.0 * document.margin - line.width) / 2.0
            }
            TextAlign::Right => {
                document.margin + document.width - 2.0 * document.margin - line.width
            }
        } + line.left;
        let line_y = document.height - document.margin - line.top - line.height;
        let line_width = line.background_width.unwrap_or(line.width);

        if let Some(color) = line.background {
            content
                .set_fill_rgb(color[0], color[1], color[2])
                .rect(document.margin + line.left, line_y, line_width, line.height)
                .fill_nonzero();
        }

        if let Some(border) = line.border {
            let color = border.color;
            let width = border.width;
            content
                .set_fill_rgb(color[0], color[1], color[2])
                .set_line_width(width);

            if border.top {
                content.rect(
                    document.margin + line.left,
                    line_y + line.height - width,
                    line_width,
                    width,
                );
            }
            if border.bottom {
                content.rect(document.margin + line.left, line_y, line_width, width);
            }
            if border.left {
                content.rect(document.margin + line.left, line_y, width, line.height);
            }
            if border.right {
                content.rect(
                    document.margin + line.left + line_width - width,
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
                let available = document.width - 2.0 * document.margin - line.left;
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

                    if run.underline {
                        let underline_y = baseline - run.size * 0.12;
                        content
                            .set_stroke_rgb(run.color[0], run.color[1], run.color[2])
                            .set_line_width(0.6)
                            .move_to(cursor, underline_y)
                            .line_to(cursor + width, underline_y)
                            .stroke();
                    }

                    cursor += width;
                }
                InlineItem::Image(image) => {
                    let top = line.top + (line.height - image.height) / 2.0;
                    let y = document.height - document.margin - top - image.height;
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
