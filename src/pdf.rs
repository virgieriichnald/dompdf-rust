use pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationType, CheckBoxState, CidFontType, FieldFlags, FieldType,
    FontFlags, PageMode, Quadding, StructRole, SystemInfo, UnicodeCmap,
};
use pdf_writer::writers::{Action, StructTreeRoot};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::css::{BackgroundRepeat, BackgroundSize, TextAlign};
use crate::layout::{
    Document, FontId, FontKind, FormControl, InlineItem, Page, StructureRole, measure,
};

struct FontRefs {
    type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    to_unicode: Ref,
}

struct OutlineNode {
    id: Ref,
    level: usize,
    title: String,
    page: usize,
    top: f32,
    parent: Option<usize>,
    prev: Option<usize>,
    next: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
}

fn build_outline_nodes(document: &Document, ids: &[Ref]) -> Vec<OutlineNode> {
    let mut nodes = document
        .bookmarks
        .iter()
        .zip(ids.iter().copied())
        .map(|(bookmark, id)| OutlineNode {
            id,
            level: bookmark.level,
            title: bookmark.title.clone(),
            page: bookmark.page,
            top: bookmark.top,
            parent: None,
            prev: None,
            next: None,
            first: None,
            last: None,
        })
        .collect::<Vec<_>>();
    let max_level = nodes.iter().map(|node| node.level).max().unwrap_or(1);
    let mut stack: Vec<usize> = Vec::new();
    let mut last_at_level = vec![None; max_level + 2];

    for index in 0..nodes.len() {
        let level = nodes[index].level;
        while stack
            .last()
            .is_some_and(|&parent| nodes[parent].level >= level)
        {
            stack.pop();
        }
        let parent = stack.last().copied();
        let prev = last_at_level[level];
        nodes[index].parent = parent;
        nodes[index].prev = prev;
        if let Some(prev) = prev {
            nodes[prev].next = Some(index);
        }
        if let Some(parent) = parent {
            if nodes[parent].first.is_none() {
                nodes[parent].first = Some(index);
            }
            nodes[parent].last = Some(index);
        }
        last_at_level[level] = Some(index);
        stack.push(index);
    }

    nodes
}

fn outline_descendant_count(nodes: &[OutlineNode], index: usize) -> usize {
    let mut count = 0;
    let mut child = nodes[index].first;
    while let Some(index) = child {
        count += 1 + outline_descendant_count(nodes, index);
        child = nodes[index].next;
    }
    count
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
    let outlines_id = Ref::new(next_id);
    next_id += 1;
    let outline_item_ids = (0..document.bookmarks.len())
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let form_count = document
        .pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .flat_map(|line| line.items.iter())
        .filter(|item| matches!(item, InlineItem::Form(_)))
        .count();
    let form_ids = (0..form_count)
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let script_action_ids = (0..document.scripts.len())
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let struct_root_id = Ref::new(next_id);
    next_id += 1;
    let struct_line_count = document
        .pages
        .iter()
        .map(|page| page.lines.len())
        .sum::<usize>();
    let struct_element_ids = (0..struct_line_count)
        .map(|_| {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        })
        .collect::<Vec<_>>();
    let outline_nodes = build_outline_nodes(document, &outline_item_ids);
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
    if !outline_nodes.is_empty() {
        let top_level = outline_nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| node.parent.is_none().then_some(index))
            .collect::<Vec<_>>();
        let mut outline = pdf.outline(outlines_id);
        outline.count(outline_nodes.len() as i32);
        if let Some(&first) = top_level.first() {
            outline.first(outline_nodes[first].id);
        }
        if let Some(&last) = top_level.last() {
            outline.last(outline_nodes[last].id);
        }
    }
    for (script, &id) in document.scripts.iter().zip(script_action_ids.iter()) {
        pdf.indirect(id)
            .start::<Action>()
            .action_type(ActionType::JavaScript)
            .js_string(TextStr(script.code.as_str()))
            .finish();
    }

    {
        let mut catalog = pdf.catalog(catalog_id);
        catalog.pages(pages_id);
        if !form_ids.is_empty() {
            catalog.form().fields(form_ids.iter().copied());
        }
        if !script_action_ids.is_empty() {
            let mut names = catalog.names();
            let mut javascript = names.javascript();
            let mut entries = javascript.names();
            for (script, &id) in document.scripts.iter().zip(script_action_ids.iter()) {
                entries.insert(Str(script.name.as_bytes()), id);
            }
        }
        if !destinations.is_empty() {
            catalog.destinations(destinations_id);
        }
        if !outline_nodes.is_empty() {
            catalog
                .outlines(outlines_id)
                .page_mode(PageMode::UseOutlines);
        }
        if !struct_element_ids.is_empty() {
            catalog.pair(Name(b"StructTreeRoot"), struct_root_id);
            catalog.mark_info().marked(true);
            if let Some(lang) = &document.lang {
                catalog.lang(TextStr(lang.as_str()));
            }
        }
    }

    if !struct_element_ids.is_empty() {
        let mut root = pdf.indirect(struct_root_id).start::<StructTreeRoot>();
        {
            let mut children = root.children();
            children.items(struct_element_ids.iter().copied());
        }
        {
            let mut parent_tree = root.parent_tree();
            let mut nums = parent_tree.nums();
            for (mcid, id) in struct_element_ids.iter().enumerate() {
                nums.insert(mcid as i32, *id);
            }
        }
        root.parent_tree_next_key(struct_element_ids.len() as i32);
        root.finish();

        let mut mcid = 0usize;
        for (page_index, page) in document.pages.iter().enumerate() {
            for line in &page.lines {
                let mut element = pdf.struct_element(struct_element_ids[mcid]);
                element
                    .kind(struct_role_kind(line.struct_role))
                    .parent(struct_root_id)
                    .page(page_ids[page_index]);
                element
                    .marked_content_child()
                    .page(page_ids[page_index])
                    .marked_content_id(mcid as i32);
                element.finish();
                mcid += 1;
            }
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

    for (index, node) in outline_nodes.iter().enumerate() {
        let mut item = pdf.outline_item(node.id);
        item.title(TextStr(node.title.as_str())).parent(
            node.parent
                .map_or(outlines_id, |index| outline_nodes[index].id),
        );
        if let Some(prev) = node.prev {
            item.prev(outline_nodes[prev].id);
        }
        if let Some(next) = node.next {
            item.next(outline_nodes[next].id);
        }
        if let Some(first) = node.first {
            item.first(outline_nodes[first].id);
        }
        if let Some(last) = node.last {
            item.last(outline_nodes[last].id);
        }
        let descendants = outline_descendant_count(&outline_nodes, index);
        if descendants > 0 {
            item.count(descendants as i32);
        }
        item.dest().page(page_ids[node.page]).xyz(
            0.0,
            document.pages[node.page].geometry.height - node.top,
            None,
        );
        item.finish();
    }

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

        let mut form_widgets = Vec::new();
        let mut form_index = 0usize;
        for line in &page.lines {
            let mut cursor = page.geometry.margin + line.left;
            for item in &line.items {
                if let InlineItem::Form(form) = item {
                    let top = line.top + (line.height - form.height) / 2.0;
                    let bottom = page.geometry.height - page.geometry.margin - top - form.height;
                    let rect = Rect::new(cursor, bottom, cursor + form.width, bottom + form.height);
                    if let Some(&id) = form_ids.get(form_index) {
                        write_form_field(&mut pdf, id, page_id, rect, &form.control);
                        form_widgets.push(id);
                        form_index += 1;
                    }
                }
                cursor += inline_width(item);
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
        if !links.is_empty() || !form_widgets.is_empty() {
            page_writer.annotations(links.into_iter().chain(form_widgets));
        }
        page_writer.finish();
    }

    pdf.pages(pages_id)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);

    let bytes = pdf.finish();
    if document.pdf_a || document.user_password.is_some() {
        postprocess_pdf(bytes, document)
    } else {
        bytes
    }
}

fn write_embedded_font(pdf: &mut Pdf, font: &crate::layout::EmbeddedFont, refs: &FontRefs) {
    let info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };

    if font.is_cff {
        pdf.stream(refs.file, font.data.as_slice())
            .pair(Name(b"Subtype"), Name(b"CIDFontType0C"))
            .finish();
    } else {
        pdf.stream(refs.file, font.data.as_slice())
            .pair(Name(b"Length1"), font.data.len() as i32)
            .finish();
    }

    let mut cmap = UnicodeCmap::<u16>::new(Name(b"Custom"), info);
    for (&ch, &glyph) in &font.glyphs {
        cmap.pair(glyph, ch);
    }
    pdf.stream(refs.to_unicode, cmap.finish().as_slice())
        .finish();

    let mut descriptor = pdf.font_descriptor(refs.descriptor);
    descriptor
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
        .stem_v(50.0);
    if font.is_cff {
        descriptor.font_file3(refs.file);
    } else {
        descriptor.font_file2(refs.file);
    }
    descriptor.finish();

    let mut cid = pdf.cid_font(refs.cid);
    cid.subtype(if font.is_cff {
        CidFontType::Type0
    } else {
        CidFontType::Type2
    })
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

fn struct_role_kind(role: StructureRole) -> StructRole {
    match role {
        StructureRole::Paragraph => StructRole::P,
        StructureRole::Heading1 => StructRole::H1,
        StructureRole::Heading2 => StructRole::H2,
        StructureRole::Heading3 => StructRole::H3,
        StructureRole::Heading4 => StructRole::H4,
        StructureRole::Heading5 => StructRole::H5,
        StructureRole::Heading6 => StructRole::H6,
        StructureRole::ListItem => StructRole::LI,
    }
}

fn struct_role_name(role: StructureRole) -> &'static [u8] {
    match role {
        StructureRole::Paragraph => b"P",
        StructureRole::Heading1 => b"H1",
        StructureRole::Heading2 => b"H2",
        StructureRole::Heading3 => b"H3",
        StructureRole::Heading4 => b"H4",
        StructureRole::Heading5 => b"H5",
        StructureRole::Heading6 => b"H6",
        StructureRole::ListItem => b"LI",
    }
}

fn inline_width(item: &InlineItem) -> f32 {
    match item {
        InlineItem::Text(run) => run.width,
        InlineItem::Image(image) => image.width,
        InlineItem::Form(form) => form.width,
    }
}

fn write_form_field(pdf: &mut Pdf, id: Ref, page_id: Ref, rect: Rect, control: &FormControl) {
    let mut field = pdf.form_field(id);
    field.partial_name(TextStr(control.field_name()));
    let mut caption = None;
    match control {
        FormControl::Text {
            value,
            password,
            multiline,
            max_len,
            required,
            read_only,
            ..
        } => {
            let mut flags = FieldFlags::empty();
            if *password {
                flags |= FieldFlags::PASSWORD;
            }
            if *multiline {
                flags |= FieldFlags::MULTILINE;
            }
            if *required {
                flags |= FieldFlags::REQUIRED;
            }
            if *read_only {
                flags |= FieldFlags::READ_ONLY;
            }
            field
                .field_type(FieldType::Text)
                .field_flags(flags)
                .text_value(TextStr(value.as_str()))
                .text_default_value(TextStr(value.as_str()))
                .vartext_quadding(Quadding::Left);
            if let Some(max_len) = max_len {
                field.text_max_len(*max_len);
            }
        }
        FormControl::CheckBox {
            checked, required, ..
        } => {
            let value = || {
                if *checked {
                    CheckBoxState::Yes
                } else {
                    CheckBoxState::Off
                }
            };
            let mut flags = FieldFlags::empty();
            if *required {
                flags |= FieldFlags::REQUIRED;
            }
            field
                .field_type(FieldType::Button)
                .field_flags(flags)
                .checkbox_value(value())
                .checkbox_default_value(value());
        }
        FormControl::Radio { value, checked, .. } => {
            let state = if *checked {
                Name(value.as_bytes())
            } else {
                Name(b"Off")
            };
            field
                .field_type(FieldType::Button)
                .field_flags(FieldFlags::RADIO | FieldFlags::NO_TOGGLE_TO_OFF)
                .radio_value(state)
                .radio_default_value(state);
        }
        FormControl::Choice {
            options,
            value,
            combo,
            multiple,
            ..
        } => {
            let mut flags = FieldFlags::empty();
            if *combo {
                flags |= FieldFlags::COMBO;
            }
            if *multiple {
                flags |= FieldFlags::MULTI_SELECT;
            }
            field.field_type(FieldType::Choice).field_flags(flags);
            field
                .choice_options()
                .options(options.iter().map(|option| TextStr(option.as_str())))
                .finish();
            field.choice_value(value.as_deref().map(TextStr));
        }
        FormControl::PushButton { caption: text, .. } => {
            caption = Some(text.clone());
            field
                .field_type(FieldType::Button)
                .field_flags(FieldFlags::PUSHBUTTON);
        }
    }

    let mut widget = field.into_annotation();
    widget
        .rect(rect)
        .page(page_id)
        .flags(AnnotationFlags::PRINT);
    if let Some(caption) = caption {
        widget
            .appearance_characteristics()
            .normal_caption(TextStr(caption.as_str()))
            .finish();
    }
    let js = match control {
        FormControl::Text { js, .. }
        | FormControl::CheckBox { js, .. }
        | FormControl::Radio { js, .. }
        | FormControl::Choice { js, .. }
        | FormControl::PushButton { js, .. } => js.as_deref(),
    };
    if let Some(js) = js {
        widget
            .action()
            .action_type(ActionType::JavaScript)
            .js_string(TextStr(js));
    }
    widget.finish();
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
    let mut push = |alpha: f32| {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha < 0.999 && !values.iter().any(|value| (*value - alpha).abs() < 0.001) {
            values.push(alpha);
        }
    };
    for page in &document.pages {
        let lines = page
            .lines
            .iter()
            .chain(page.overlays.iter().map(|overlay| &overlay.line));
        for line in lines {
            push(line.opacity);
            push(line.background_alpha);
            if let Some(border) = line.border {
                push(border.alpha);
            }
            for item in &line.items {
                if let InlineItem::Text(run) = item {
                    push(run.alpha);
                }
            }
        }
    }
    values
}

fn absolute_background_offset(value: f32, available: f32) -> f32 {
    if value >= 0.0 {
        value.min(available)
    } else {
        (available + value).max(0.0)
    }
}

fn rounded_rect_path(
    content: &mut Content,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radii: [f32; 4],
) {
    let limit = width.min(height) * 0.5;
    let [mut tl, mut tr, mut br, mut bl] = radii.map(|radius| radius.max(0.0).min(limit));
    if tl + tr > width {
        let scale = width / (tl + tr);
        tl *= scale;
        tr *= scale;
    }
    if bl + br > width {
        let scale = width / (bl + br);
        bl *= scale;
        br *= scale;
    }
    if tl + bl > height {
        let scale = height / (tl + bl);
        tl *= scale;
        bl *= scale;
    }
    if tr + br > height {
        let scale = height / (tr + br);
        tr *= scale;
        br *= scale;
    }
    if [tl, tr, br, bl].iter().all(|radius| *radius == 0.0) {
        content.rect(x, y, width, height);
        return;
    }

    let ktl = tl * 0.552_284_8;
    let ktr = tr * 0.552_284_8;
    let kbr = br * 0.552_284_8;
    let kbl = bl * 0.552_284_8;
    content
        .move_to(x + tl, y)
        .line_to(x + width - tr, y)
        .cubic_to(
            x + width - tr + ktr,
            y,
            x + width,
            y + tr - ktr,
            x + width,
            y + tr,
        )
        .line_to(x + width, y + height - br)
        .cubic_to(
            x + width,
            y + height - br + kbr,
            x + width - br + kbr,
            y + height,
            x + width - br,
            y + height,
        )
        .line_to(x + bl, y + height)
        .cubic_to(
            x + bl - kbl,
            y + height,
            x,
            y + height - bl + kbl,
            x,
            y + height - bl,
        )
        .line_to(x, y + tl)
        .cubic_to(x, y + tl - ktl, x + tl - ktl, y, x + tl, y)
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

    let mcid_base = document.pages[..page_index]
        .iter()
        .map(|page| page.lines.len())
        .sum::<usize>();
    let mut drawable = page
        .lines
        .iter()
        .map(|line| (0, false, line))
        .chain(
            page.overlays
                .iter()
                .map(|overlay| (overlay.z, true, &overlay.line)),
        )
        .collect::<Vec<_>>();
    drawable.sort_by_key(|(z, _, _)| *z);

    let state_for = |alpha: f32| {
        if alpha >= 0.999 {
            None
        } else {
            opacities
                .iter()
                .position(|value| (value - alpha).abs() < 0.001)
        }
    };

    let mut regular_index = 0usize;
    for (_, is_overlay, line) in drawable {
        let mcid = (!is_overlay).then(|| mcid_base + regular_index);
        if mcid.is_some() {
            regular_index += 1;
        }
        if let Some(mcid) = mcid {
            let mut marked = content
                .begin_marked_content_with_properties(Name(struct_role_name(line.struct_role)));
            marked.properties().identify(mcid as i32);
            marked.finish();
        }

        let alpha_state = state_for(line.opacity);
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
                BackgroundSize::Dimensions(width, height) => (width, height),
            };
            if tile_width > 0.0 && tile_height > 0.0 {
                let image_alpha = state_for(line.background_alpha);
                if let Some(state) = image_alpha {
                    content.save_state();
                    let name = format!("GS{}", state + 1);
                    content.op("gs").operand(Name(name.as_bytes()));
                }
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
                let repeats = line.background_repeat != BackgroundRepeat::NoRepeat;
                let left_edge = margin + line.left;
                let right_edge = left_edge + line_width;
                let available_x = (line_width - tile_width).max(0.0);
                let available_y = (line.height - tile_height).max(0.0);
                let pos_x = if line.background_position_x_is_length {
                    absolute_background_offset(line.background_position_x, available_x)
                } else {
                    line.background_position_x.clamp(0.0, 1.0) * available_x
                };
                let pos_y = if line.background_position_y_is_length {
                    absolute_background_offset(line.background_position_y, available_y)
                } else {
                    line.background_position_y.clamp(0.0, 1.0) * available_y
                };
                let start_top = line_y + line.height - pos_y;
                let mut bottom = if repeats {
                    start_top
                } else {
                    start_top - tile_height
                };
                while bottom + tile_height > line_y {
                    let mut x_tile = if repeats {
                        left_edge + pos_x - tile_width
                    } else {
                        left_edge + pos_x
                    };
                    while x_tile < right_edge {
                        content
                            .transform([tile_width, 0.0, 0.0, tile_height, x_tile, bottom])
                            .x_object(Name(name.as_bytes()));
                        if !repeats {
                            break;
                        }
                        x_tile += tile_width;
                    }
                    if !repeats {
                        break;
                    }
                    bottom -= tile_height;
                }
                content.restore_state();
                if image_alpha.is_some() {
                    content.restore_state();
                }
            }
        }

        if let Some(color) = line.background {
            let background_alpha = state_for(line.background_alpha);
            if let Some(state) = background_alpha {
                content.save_state();
                let name = format!("GS{}", state + 1);
                content.op("gs").operand(Name(name.as_bytes()));
            }
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
            if background_alpha.is_some() {
                content.restore_state();
            }
        }

        if let Some(border) = line.border {
            let color = border.color;
            let width = border.width;
            let border_alpha = state_for(border.alpha);
            if let Some(state) = border_alpha {
                content.save_state();
                let name = format!("GS{}", state + 1);
                content.op("gs").operand(Name(name.as_bytes()));
            }
            content
                .set_fill_rgb(color[0], color[1], color[2])
                .set_line_width(width);

            let rounded = line.border_radius.iter().any(|radius| *radius > 0.0)
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
            if border_alpha.is_some() {
                content.restore_state();
            }
        }

        let max_size = line
            .items
            .iter()
            .filter_map(|item| match item {
                InlineItem::Text(run) => Some(run.size),
                InlineItem::Image(_) | InlineItem::Form(_) => None,
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
                    let run_alpha = state_for(run.alpha);
                    if let Some(state) = run_alpha {
                        content.save_state();
                        let name = format!("GS{}", state + 1);
                        content.op("gs").operand(Name(name.as_bytes()));
                    }

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

                    if run_alpha.is_some() {
                        content.restore_state();
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
                InlineItem::Form(form) => {
                    let top = line.top + (line.height - form.height) / 2.0;
                    let y = height - margin - top - form.height;
                    let text = form.control.display_text();
                    let bytes = to_win_ansi(&text);

                    content
                        .save_state()
                        .set_stroke_rgb(0.35, 0.35, 0.35)
                        .set_line_width(0.7)
                        .rect(cursor, y, form.width, form.height)
                        .stroke();
                    if !bytes.is_empty() {
                        content
                            .begin_text()
                            .set_text_matrix([1.0, 0.0, 0.0, 1.0, cursor + 3.0, y + 4.0])
                            .set_font(Name(b"F1"), 9.0)
                            .set_fill_rgb(0.0, 0.0, 0.0)
                            .show(Str(bytes.as_slice()))
                            .end_text();
                    }
                    content.restore_state();

                    cursor += form.width;
                }
            }
        }

        if alpha_state.is_some() {
            content.restore_state();
        }
        if mcid.is_some() {
            content.end_marked_content();
        }
    }

    if let Some(template) = &document.page_text {
        let text = template
            .replace("{PAGE_NUM}", &(page_index + 1).to_string())
            .replace("{PAGE_COUNT}", &page_count.to_string());
        let bytes = to_win_ansi(&text);
        if !bytes.is_empty() {
            let text_width = measure(
                &text,
                9.0,
                FontId::Builtin(FontKind::Regular),
                &document.fonts,
            );
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

#[derive(Clone)]
struct PdfObject {
    id: u32,
    body: Vec<u8>,
}

struct TrailerInfo {
    size: u32,
    root: u32,
    info: Option<u32>,
}

fn postprocess_pdf(mut bytes: Vec<u8>, document: &Document) -> Vec<u8> {
    let (mut prefix, mut objects, trailer) = match parse_pdf_parts(&bytes) {
        Some(parts) => parts,
        None => return bytes,
    };
    let trailer_info = match parse_trailer(&trailer) {
        Some(info) => info,
        None => return bytes,
    };
    let mut size = trailer_info.size;
    if document.pdf_a && prefix.starts_with(b"%PDF-1.") {
        prefix[..8].copy_from_slice(b"%PDF-1.4");
    }

    let mut extras = Vec::new();
    let mut output_intent_id = None;
    let mut metadata_id = None;
    if document.pdf_a {
        output_intent_id = Some(size);
        size += 1;
        metadata_id = Some(size);
        size += 1;
    }
    let encrypt_id = if document.user_password.is_some() {
        let id = size;
        size += 1;
        Some(id)
    } else {
        None
    };

    let user_password = document.user_password.as_deref().unwrap_or("");
    let owner_password = document.owner_password.as_deref().unwrap_or(user_password);
    let file_id = md5(&[bytes.as_slice(), user_password.as_bytes()].concat());
    let encryption =
        encrypt_id.map(|id| build_encryption(user_password, owner_password, &file_id, id));

    if let Some(id) = metadata_id {
        let xmp = build_xmp(document);
        let mut body = format!(
            "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n",
            xmp.len()
        )
        .into_bytes();
        body.extend_from_slice(&xmp);
        body.extend_from_slice(b"\nendstream\n");
        extras.push(PdfObject { id, body });
    }
    if let Some(id) = output_intent_id {
        extras.push(PdfObject {
            id,
            body: b"<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB IEC61966-2.1) /RegistryName (http://www.color.org) /Info (sRGB IEC61966-2.1) >>\n".to_vec(),
        });
    }

    if let Some(metadata) = metadata_id {
        let entry = format!("/Metadata {metadata} 0 R\n");
        insert_dict_entry(&mut objects, trailer_info.root, &entry);
    }
    if let Some(lang) = &document.lang {
        let entry = format!("/Lang ({})\n", pdf_escape(lang));
        insert_dict_entry(&mut objects, trailer_info.root, &entry);
    }

    if let Some(encryption) = &encryption {
        for object in objects.iter_mut().chain(extras.iter_mut()) {
            if object.id == encryption.object_id {
                continue;
            }
            object.body = encrypt_object(&object.body, &encryption.file_key, object.id);
        }
    }
    objects.extend(extras);
    if let Some(encryption) = &encryption {
        objects.push(PdfObject {
            id: encryption.object_id,
            body: encryption.body.clone(),
        });
    }
    objects.sort_by_key(|object| object.id);

    let mut out = prefix;
    let mut offsets = vec![0usize; size as usize];
    for object in &objects {
        offsets[object.id as usize] = out.len();
        out.extend_from_slice(format!("{} 0 obj\n", object.id).as_bytes());
        out.extend_from_slice(&object.body);
        out.extend_from_slice(b"endobj\n");
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", size).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        if *offset == 0 {
            out.extend_from_slice(b"0000000000 65535 f \n");
        } else {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
    }
    out.extend_from_slice(b"trailer\n<<\n");
    out.extend_from_slice(format!("/Size {}\n", size).as_bytes());
    out.extend_from_slice(format!("/Root {} 0 R\n", trailer_info.root).as_bytes());
    if let Some(info) = trailer_info.info {
        out.extend_from_slice(format!("/Info {info} 0 R\n").as_bytes());
    }
    if let Some(id) = output_intent_id {
        out.extend_from_slice(format!("/OutputIntents [{id} 0 R]\n").as_bytes());
    }
    if let Some(encryption) = &encryption {
        out.extend_from_slice(format!("/Encrypt {} 0 R\n", encryption.object_id).as_bytes());
        let id = hex_string(&file_id);
        out.extend_from_slice(format!("/ID [<{id}><{id}>]\n").as_bytes());
    }
    out.extend_from_slice(b">>\nstartxref\n");
    out.extend_from_slice(format!("{xref_offset}\n%%EOF\n").as_bytes());
    bytes = out;
    bytes
}

struct Encryption {
    object_id: u32,
    file_key: Vec<u8>,
    body: Vec<u8>,
}

fn build_encryption(
    user_password: &str,
    owner_password: &str,
    file_id: &[u8; 16],
    object_id: u32,
) -> Encryption {
    let password_pad = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let padded_user = pad_password(user_password, &password_pad);
    let padded_owner = pad_password(owner_password, &password_pad);
    let owner_digest = md5(&padded_owner);
    let owner_entry = rc4(&padded_user, &owner_digest[..5]);
    let mut key_input = Vec::new();
    key_input.extend_from_slice(&padded_user);
    key_input.extend_from_slice(&owner_entry);
    key_input.extend_from_slice(&(-1i32).to_le_bytes());
    key_input.extend_from_slice(file_id);
    let file_key = md5(&key_input)[..5].to_vec();
    let user_entry = rc4(&padded_user, &file_key);
    let body = format!(
        "<< /Filter /Standard /V 1 /R 2 /Length 40 /P -1 /O <{}> /U <{}> >>\n",
        hex_string(&owner_entry),
        hex_string(&user_entry)
    )
    .into_bytes();
    Encryption {
        object_id,
        file_key,
        body,
    }
}

fn pad_password(password: &str, pad: &[u8; 32]) -> Vec<u8> {
    let mut bytes = password.as_bytes().to_vec();
    bytes.truncate(32);
    bytes.extend_from_slice(&pad[..32 - bytes.len()]);
    bytes
}

fn object_key(file_key: &[u8], id: u32) -> Vec<u8> {
    let mut input = Vec::new();
    input.extend_from_slice(file_key);
    input.extend_from_slice(&id.to_le_bytes()[..3]);
    input.extend_from_slice(&0u16.to_le_bytes());
    md5(&input)[..10].to_vec()
}

fn encrypt_object(body: &[u8], file_key: &[u8], id: u32) -> Vec<u8> {
    let key = object_key(file_key, id);
    if let Some(stream_pos) = find_subslice(body, b"stream") {
        let mut data_start = stream_pos + 6;
        if body.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if body.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        if let Some(rel) = find_subslice(&body[data_start..], b"endstream") {
            let stream_end = data_start + rel;
            let mut data_end = stream_end;
            if data_end > data_start && body[data_end - 1] == b'\n' {
                data_end -= 1;
            }
            if data_end > data_start && body[data_end - 1] == b'\r' {
                data_end -= 1;
            }
            let eol_start = data_end;
            let mut out = encrypt_pdf_strings(&body[..stream_pos], &key);
            out.extend_from_slice(&body[stream_pos..data_start]);
            out.extend_from_slice(&rc4(&body[data_start..eol_start], &key));
            out.extend_from_slice(&body[eol_start..]);
            return out;
        }
    }
    encrypt_pdf_strings(body, &key)
}

fn encrypt_pdf_strings(body: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        if body[i] == b'(' {
            let (raw, end) = decode_literal_string(body, i);
            out.extend_from_slice(b"<");
            out.extend_from_slice(&hex(&rc4(&raw, key)));
            out.extend_from_slice(b">");
            i = end;
        } else if body[i] == b'<' && body.get(i + 1) != Some(&b'<') {
            if let Some(rel) = body[i..].iter().position(|byte| *byte == b'>') {
                let end = i + rel;
                let raw = decode_hex_string(&body[i + 1..end]);
                out.extend_from_slice(b"<");
                out.extend_from_slice(&hex(&rc4(&raw, key)));
                out.extend_from_slice(b">");
                i = end + 1;
                continue;
            }
            out.push(body[i]);
            i += 1;
        } else {
            out.push(body[i]);
            i += 1;
        }
    }
    out
}

fn decode_literal_string(body: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut raw = Vec::new();
    let mut depth = 1usize;
    let mut i = start + 1;
    while i < body.len() {
        match body[i] {
            b'\\' => {
                i += 1;
                if i >= body.len() {
                    break;
                }
                match body[i] {
                    b'n' => raw.push(b'\n'),
                    b'r' => raw.push(b'\r'),
                    b't' => raw.push(b'\t'),
                    b'b' => raw.push(0x08),
                    b'f' => raw.push(0x0c),
                    b'\r' => {
                        if body.get(i + 1) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {}
                    digit @ b'0'..=b'7' => {
                        let mut value = (digit - b'0') as u32;
                        let mut count = 1;
                        while count < 3 {
                            match body.get(i + 1) {
                                Some(next @ b'0'..=b'7') => {
                                    value = value * 8 + (next - b'0') as u32;
                                    i += 1;
                                    count += 1;
                                }
                                _ => break,
                            }
                        }
                        raw.push(value as u8);
                    }
                    other => raw.push(other),
                }
                i += 1;
            }
            b'(' => {
                depth += 1;
                raw.push(b'(');
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    break;
                }
                raw.push(b')');
            }
            other => {
                raw.push(other);
                i += 1;
            }
        }
    }
    (raw, i)
}

fn decode_hex_string(body: &[u8]) -> Vec<u8> {
    let mut digits = Vec::new();
    for &byte in body {
        let value = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => continue,
        };
        digits.push(value);
    }
    if digits.len() % 2 == 1 {
        digits.push(0);
    }
    digits
        .chunks(2)
        .map(|pair| pair[0] * 16 + pair[1])
        .collect()
}

fn build_xmp(document: &Document) -> Vec<u8> {
    let title = document.metadata.title.as_deref().unwrap_or("Document");
    let author = document.metadata.author.as_deref().unwrap_or("");
    let subject = document.metadata.subject.as_deref().unwrap_or("");
    let creator = "html2pdf-rs";
    format!(
        "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></dc:title>\
         <dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>\
         <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{subject}</rdf:li></rdf:Alt></dc:description>\
         </rdf:Description>\
         <rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" \
         xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\
         <pdf:Producer>{creator}</pdf:Producer>\
         <pdfaid:part>1</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>\
         </rdf:Description>\
         </rdf:RDF></x:xmpmeta>\
         <?xpacket end=\"w\"?>"
    )
    .into_bytes()
}

fn pdf_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

fn parse_pdf_parts(bytes: &[u8]) -> Option<(Vec<u8>, Vec<PdfObject>, Vec<u8>)> {
    let mut objects = Vec::new();
    let mut pos = 0usize;
    let mut prefix_end = 0usize;
    while let Some(rel) = find_subslice(&bytes[pos..], b" obj") {
        let marker = pos + rel;
        let line_start = bytes[..marker]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let header = String::from_utf8_lossy(&bytes[line_start..marker]);
        let mut fields = header.split_whitespace();
        let id = fields.next()?.parse::<u32>().ok()?;
        let generation = fields.next()?.parse::<u32>().ok()?;
        if generation != 0 {
            pos = marker + 4;
            continue;
        }
        let body_start = marker + 4;
        let end_rel = find_subslice(&bytes[body_start..], b"endobj")?;
        let body_end = body_start + end_rel;
        let body = bytes[body_start..body_end].to_vec();
        if objects.is_empty() {
            prefix_end = line_start;
        }
        objects.push(PdfObject { id, body });
        pos = body_end + 6;
    }
    if objects.is_empty() {
        return None;
    }
    let trailer_start = pos + find_subslice(&bytes[pos..], b"trailer")?;
    let dict_start = trailer_start + 7;
    let dict_rel = find_subslice(&bytes[dict_start..], b"startxref")?;
    let dict_end = dict_start + dict_rel;
    Some((
        bytes[..prefix_end].to_vec(),
        objects,
        bytes[dict_start..dict_end].to_vec(),
    ))
}

fn parse_trailer(dict: &[u8]) -> Option<TrailerInfo> {
    fn value_after<'a>(dict: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
        let pos = find_subslice(dict, key)? + key.len();
        let rest = &dict[pos..];
        let start = rest.iter().position(|byte| !byte.is_ascii_whitespace())?;
        let end = rest[start..]
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
            .map(|index| start + index)
            .unwrap_or(rest.len());
        Some(&rest[start..end])
    }
    let size = std::str::from_utf8(value_after(dict, b"/Size")?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    fn reference(dict: &[u8], key: &[u8]) -> Option<u32> {
        let value = value_after(dict, key)?;
        let text = std::str::from_utf8(value).ok()?;
        text.split_whitespace().next()?.parse::<u32>().ok()
    }
    Some(TrailerInfo {
        size,
        root: reference(dict, b"/Root")?,
        info: reference(dict, b"/Info"),
    })
}

fn insert_dict_entry(objects: &mut [PdfObject], id: u32, entry: &str) {
    if let Some(object) = objects.iter_mut().find(|object| object.id == id)
        && let Some(pos) = object.body.iter().rposition(|byte| *byte == b'>')
    {
        object
            .body
            .splice(pos..pos, entry.as_bytes().iter().copied());
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn hex(bytes: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize]);
        out.push(DIGITS[(byte & 0x0f) as usize]);
    }
    out
}

fn hex_string(bytes: &[u8]) -> String {
    String::from_utf8(hex(bytes)).unwrap_or_default()
}

fn rc4(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut state: Vec<u8> = (0..=255).collect();
    let mut j = 0usize;
    for i in 0..256 {
        j = (j + state[i] as usize + key[i % key.len()] as usize) & 0xff;
        state.swap(i, j);
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0usize;
    j = 0usize;
    for &byte in data {
        i = (i + 1) & 0xff;
        j = (j + state[i] as usize) & 0xff;
        state.swap(i, j);
        let k = state[(state[i] as usize + state[j] as usize) & 0xff];
        out.push(byte ^ k);
    }
    out
}

fn md5(data: &[u8]) -> [u8; 16] {
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_le_bytes());

    let shifts = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;

    let (chunks, _) = message.as_chunks::<64>();
    for chunk in chunks {
        let mut words = [0u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_le_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for (index, shift) in shifts.iter().copied().enumerate() {
            let (f, g) = match index {
                0..=15 => ((b & c) | (!b & d), index),
                16..=31 => ((d & b) | (!d & c), (5 * index + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | !d), (7 * index) % 16),
            };
            let constant = (((index + 1) as f64).sin().abs() * 4_294_967_296.0).floor() as u32;
            let next = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(constant)
                    .wrapping_add(words[g])
                    .rotate_left(shift),
            );
            a = d;
            d = c;
            c = b;
            b = next;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_known_digest() {
        assert_eq!(hex_string(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn rc4_matches_known_ciphertext() {
        assert_eq!(hex_string(&rc4(b"Plaintext", b"Key")), "bbf316e8d940af0ad3");
    }

    #[test]
    fn encrypt_pdf_strings_round_trip() {
        let key = b"secretkey";
        let encrypted = encrypt_pdf_strings(b"(halo)", key);
        let text = String::from_utf8(encrypted).unwrap();
        let body = text.trim_start_matches('<').trim_end_matches('>');
        let mut digits = Vec::new();
        for pair in body.as_bytes().chunks(2) {
            digits.push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap());
        }
        assert_eq!(rc4(&digits, key), b"halo");
    }
}
