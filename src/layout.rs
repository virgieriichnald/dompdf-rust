use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use scraper::{ElementRef, Html, Selector};
use ttf_parser::Face;

use crate::css::{
    BorderSpec, Display, FontFace, Rule, Style, TextAlign, parse_font_faces, parse_styles,
    style_for,
};

const PAGE_WIDTH: f32 = 595.28;
const PAGE_HEIGHT: f32 = 841.89;
const PAGE_MARGIN: f32 = 54.0;

const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    Regular,
    Bold,
    Oblique,
    BoldOblique,
}

impl FontKind {
    pub fn from_style(style: &Style) -> Self {
        match (style.bold, style.italic) {
            (true, true) => Self::BoldOblique,
            (true, false) => Self::Bold,
            (false, true) => Self::Oblique,
            (false, false) => Self::Regular,
        }
    }

    pub fn resource_name(self) -> &'static [u8] {
        match self {
            Self::Regular => b"F1",
            Self::Bold => b"F2",
            Self::Oblique => b"F3",
            Self::BoldOblique => b"F4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontId {
    Builtin(FontKind),
    Embedded(usize),
}

#[derive(Clone, Debug)]
pub struct Run {
    pub text: String,
    pub font: FontId,
    pub size: f32,
    pub color: [f32; 3],
    pub underline: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ImageItem {
    pub image: usize,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub enum InlineItem {
    Text(Run),
    Image(ImageItem),
}

#[derive(Clone, Debug)]
pub struct LinkRect {
    pub x: f32,
    pub width: f32,
    pub uri: String,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub items: Vec<InlineItem>,
    pub width: f32,
    pub height: f32,
    pub space_before: f32,
    pub space_after: f32,
    pub top: f32,
    pub left: f32,
    pub align: TextAlign,
    pub background: Option<[f32; 3]>,
    pub background_width: Option<f32>,
    pub border: Option<BorderSpec>,
    pub links: Vec<LinkRect>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug)]
pub struct ImageAsset {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct EmbeddedFont {
    pub family: String,
    pub data: Vec<u8>,
    pub advance: HashMap<char, f32>,
    pub glyphs: HashMap<char, u16>,
    pub units_per_em: u16,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub pages: Vec<Page>,
    pub images: Vec<ImageAsset>,
    pub fonts: Vec<EmbeddedFont>,
    pub width: f32,
    pub height: f32,
    pub margin: f32,
}

impl Document {
    pub fn from_html(html: &str) -> Self {
        Self::from_html_with_base(html, Path::new("."))
    }

    pub fn from_html_with_base(html: &str, base: &Path) -> Self {
        let parsed = Html::parse_document(html);
        let rules = parse_styles(&parsed);
        let font_faces = parse_font_faces(&parsed);
        let base_style = Style::default();
        let mut collector = Collector::new(
            PAGE_WIDTH - 2.0 * PAGE_MARGIN,
            PAGE_HEIGHT - 2.0 * PAGE_MARGIN,
            base,
        );

        collector.load_fonts(&font_faces);
        walk(parsed.root_element(), &base_style, &rules, &mut collector);
        collector.flush_line();

        Document {
            pages: collector.pages,
            images: collector.images,
            fonts: collector.fonts,
            width: PAGE_WIDTH,
            height: PAGE_HEIGHT,
            margin: PAGE_MARGIN,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ListState {
    ordered: bool,
    counter: usize,
}

struct Collector {
    pages: Vec<Page>,
    current: Vec<InlineItem>,
    width: f32,
    height: f32,
    pending_before: f32,
    align: TextAlign,
    max_width: f32,
    page_height: f32,
    cursor: f32,
    base: PathBuf,
    images: Vec<ImageAsset>,
    image_cache: HashMap<String, Option<usize>>,
    fonts: Vec<EmbeddedFont>,
    font_cache: HashMap<String, Option<usize>>,
    background: Option<[f32; 3]>,
    border: Option<BorderSpec>,
    style_stack: Vec<(Option<[f32; 3]>, Option<BorderSpec>)>,
    link: Option<String>,
    lists: Vec<ListState>,
}

impl Collector {
    fn new(max_width: f32, page_height: f32, base: &Path) -> Self {
        Self {
            pages: vec![Page { lines: Vec::new() }],
            current: Vec::new(),
            width: 0.0,
            height: 0.0,
            pending_before: 0.0,
            align: TextAlign::Left,
            max_width,
            page_height,
            cursor: 0.0,
            base: base.to_path_buf(),
            images: Vec::new(),
            image_cache: HashMap::new(),
            fonts: Vec::new(),
            font_cache: HashMap::new(),
            background: None,
            border: None,
            style_stack: Vec::new(),
            link: None,
            lists: Vec::new(),
        }
    }

    fn new_page(&mut self) {
        self.pages.push(Page { lines: Vec::new() });
        self.cursor = 0.0;
    }

    fn page_is_empty(&self) -> bool {
        self.pages.last().is_some_and(|page| page.lines.is_empty())
    }

    fn push_line(&mut self, mut line: Line) {
        if line.background.is_none() {
            line.background = self.background;
        }
        if line.border.is_none() {
            line.border = self.border;
        }
        if line.background_width.is_none() && self.background.is_some() {
            line.background_width = Some(self.max_width);
        }
        if line.links.is_empty() {
            line.links = collect_links(&line.items, &self.fonts);
        }
        self.pages.last_mut().expect("page exists").lines.push(line);
    }

    fn emit_line(&mut self, mut line: Line) {
        let needed = line.space_before + line.height + line.space_after;
        if self.cursor + needed > self.page_height && !self.page_is_empty() {
            self.new_page();
            line.space_before = 0.0;
        }

        self.cursor += line.space_before;
        line.top = self.cursor;
        line.left = 0.0;
        self.cursor += line.height + line.space_after;
        self.push_line(line);
    }

    fn reserve(&mut self, height: f32, space_before: f32) -> f32 {
        if self.cursor + space_before + height > self.page_height && !self.page_is_empty() {
            self.new_page();
        }
        self.cursor += space_before;
        let top = self.cursor;
        self.cursor += height;
        top
    }

    fn flush_line(&mut self) {
        if self.current.is_empty() {
            return;
        }

        let line = Line {
            items: std::mem::take(&mut self.current),
            width: self.width,
            height: self.height.max(12.0),
            space_before: self.pending_before,
            space_after: 0.0,
            top: 0.0,
            left: 0.0,
            align: self.align,
            background: None,
            background_width: None,
            border: None,
            links: Vec::new(),
        };

        self.width = 0.0;
        self.height = 0.0;
        self.pending_before = 0.0;
        self.emit_line(line);
    }

    fn start_block(&mut self, style: &Style) {
        self.flush_line();
        self.style_stack.push((self.background, self.border));
        if style.background.is_some() {
            self.background = style.background;
        }
        if style.border.is_some() {
            self.border = style.border;
        }
        self.pending_before = self.pending_before.max(style.margin_top_pt());
        self.align = style.text_align;
    }

    fn end_block(&mut self, style: &Style) {
        self.flush_line();
        let margin = style.margin_bottom_pt();
        if let Some(last) = self.pages.last_mut().and_then(|page| page.lines.last_mut()) {
            last.space_after = last.space_after.max(margin);
        }
        self.cursor += margin;
        if let Some((background, border)) = self.style_stack.pop() {
            self.background = background;
            self.border = border;
        }
    }

    fn font_id(&self, style: &Style) -> FontId {
        let family = style
            .font_family
            .split(',')
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches(|ch| ch == '"' || ch == '\'')
            .to_ascii_lowercase();

        self.fonts
            .iter()
            .position(|font| font.family.to_ascii_lowercase() == family)
            .map(FontId::Embedded)
            .unwrap_or(FontId::Builtin(FontKind::from_style(style)))
    }

    fn text_width(&self, text: &str, style: &Style) -> f32 {
        measure(text, style.font_size, self.font_id(style), &self.fonts)
    }

    fn push_text(&mut self, text: &str, style: &Style) {
        let space_width = self.text_width(" ", style);

        for word in text.split_whitespace() {
            let word_width = self.text_width(word, style);
            let extra = if self.current.is_empty() {
                0.0
            } else {
                space_width
            };

            if !self.current.is_empty() && self.width + extra + word_width > self.max_width {
                self.flush_line();
            }

            if !self.current.is_empty() {
                self.push_run(" ", style, space_width);
            }

            self.push_run(word, style, word_width);
            self.height = self.height.max(style.font_size * style.line_height);
        }
    }

    fn push_run(&mut self, text: &str, style: &Style, width: f32) {
        self.current.push(InlineItem::Text(Run {
            text: text.to_string(),
            font: self.font_id(style),
            size: style.font_size,
            color: style.color,
            underline: style.underline,
            link: self.link.clone(),
        }));
        self.width += width;
    }

    fn push_image(
        &mut self,
        src: &str,
        alt: &str,
        attr_width: Option<f32>,
        attr_height: Option<f32>,
        style: &Style,
    ) {
        let key = src.to_string();
        let index = if let Some(cached) = self.image_cache.get(&key) {
            *cached
        } else {
            let loaded = read_resource(src, &self.base)
                .and_then(|bytes| {
                    if src.to_ascii_lowercase().ends_with(".svg") {
                        String::from_utf8(bytes)
                            .ok()
                            .and_then(|text| render_svg(&text))
                    } else {
                        load_image_bytes(&bytes)
                    }
                })
                .map(|asset| {
                    self.images.push(asset);
                    self.images.len() - 1
                });
            self.image_cache.insert(key, loaded);
            loaded
        };

        let Some(index) = index else {
            self.push_text(alt, style);
            return;
        };

        self.push_image_item(index, attr_width, attr_height, style);
    }

    fn push_image_item(
        &mut self,
        index: usize,
        attr_width: Option<f32>,
        attr_height: Option<f32>,
        style: &Style,
    ) {
        let asset = &self.images[index];
        let natural_width = asset.width as f32 * 0.75;
        let natural_height = asset.height as f32 * 0.75;
        let (width, height) = match (attr_width, attr_height) {
            (Some(width), Some(height)) => (width, height),
            (Some(width), None) => (width, width * natural_height / natural_width),
            (None, Some(height)) => (height * natural_width / natural_height, height),
            (None, None) => (natural_width, natural_height),
        };

        if !self.current.is_empty() && self.width + width > self.max_width {
            self.flush_line();
        }

        self.current.push(InlineItem::Image(ImageItem {
            image: index,
            width,
            height,
        }));
        self.width += width;
        self.height = self
            .height
            .max(height)
            .max(style.font_size * style.line_height);
    }

    fn load_fonts(&mut self, faces: &[FontFace]) {
        for face in faces {
            let key = face.family.to_ascii_lowercase();
            if !self.font_cache.contains_key(&key) {
                let loaded = read_resource(&face.src, &self.base)
                    .and_then(|bytes| load_font(&face.family, &bytes))
                    .map(|font| {
                        self.fonts.push(font);
                        self.fonts.len() - 1
                    });
                self.font_cache.insert(key, loaded);
            }
        }
    }
}

fn read_resource(src: &str, base: &Path) -> Option<Vec<u8>> {
    if let Some(position) = src.find(";base64,") {
        return base64::engine::general_purpose::STANDARD
            .decode(&src[position + ";base64,".len()..])
            .ok();
    }

    if src.starts_with("http://") || src.starts_with("https://") {
        return None;
    }

    fs::read(base.join(src)).ok()
}

fn load_image_bytes(bytes: &[u8]) -> Option<ImageAsset> {
    let image = image::load_from_memory(bytes).ok()?.to_rgb8();
    let (width, height) = image.dimensions();

    Some(ImageAsset {
        width,
        height,
        pixels: image.into_raw(),
    })
}

fn render_svg(svg: &str) -> Option<ImageAsset> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &options).ok()?;
    let width = tree.size().width().max(1.0) as u32;
    let height = tree.size().height().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    load_image_bytes(&pixmap.encode_png().ok()?)
}

fn load_font(family: &str, bytes: &[u8]) -> Option<EmbeddedFont> {
    let face = Face::parse(bytes, 0).ok()?;
    let units_per_em = face.units_per_em();
    let mut advance = HashMap::new();
    let mut glyphs = HashMap::new();

    for code in 0x20u32..=0x2ff {
        let Some(ch) = char::from_u32(code) else {
            continue;
        };
        let Some(glyph) = face.glyph_index(ch) else {
            continue;
        };
        let width = face.glyph_hor_advance(glyph).unwrap_or(0) as f32 / units_per_em as f32;
        advance.insert(ch, width);
        glyphs.insert(ch, glyph.0);
    }

    Some(EmbeddedFont {
        family: family.to_string(),
        data: bytes.to_vec(),
        advance,
        glyphs,
        units_per_em,
        ascent: face.ascender() as f32 / units_per_em as f32,
        descent: face.descender() as f32 / units_per_em as f32,
    })
}

fn collect_links(items: &[InlineItem], fonts: &[EmbeddedFont]) -> Vec<LinkRect> {
    let mut links = Vec::new();
    let mut cursor = 0.0;
    let mut active: Option<(String, f32, f32)> = None;

    for item in items {
        let InlineItem::Text(run) = item else {
            if let Some((uri, x, width)) = active.take() {
                links.push(LinkRect { x, width, uri });
            }
            continue;
        };

        let width = measure(&run.text, run.size, run.font, fonts);
        match (&run.link, active.as_mut()) {
            (Some(uri), Some((current_uri, _, current_width))) if uri == current_uri => {
                *current_width += width;
            }
            (Some(uri), _) => {
                if let Some((old_uri, x, old_width)) = active.take() {
                    links.push(LinkRect {
                        x,
                        width: old_width,
                        uri: old_uri,
                    });
                }
                active = Some((uri.clone(), cursor, width));
            }
            (None, _) => {
                if let Some((old_uri, x, old_width)) = active.take() {
                    links.push(LinkRect {
                        x,
                        width: old_width,
                        uri: old_uri,
                    });
                }
            }
        }
        cursor += width;
    }

    if let Some((uri, x, width)) = active {
        links.push(LinkRect { x, width, uri });
    }

    links
}

fn walk(element: ElementRef, parent: &Style, rules: &[Rule], collector: &mut Collector) {
    let tag = element.value().name();

    if matches!(
        tag,
        "head" | "title" | "meta" | "link" | "script" | "style" | "noscript" | "template"
    ) {
        return;
    }

    let mut style = style_for(&element, parent, rules);

    if tag == "table"
        && element
            .attr("border")
            .is_some_and(|value| value.parse::<u32>().unwrap_or(0) > 0)
    {
        style.border = Some(BorderSpec {
            width: 1.0,
            color: [0.0, 0.0, 0.0],
            top: true,
            right: true,
            bottom: true,
            left: true,
        });
    }

    if style.display == Display::None {
        return;
    }

    let previous_link = collector.link.clone();
    if tag == "a" {
        collector.link = element.attr("href").map(str::to_string);
    }

    if tag == "br" {
        collector.flush_line();
        collector.link = previous_link;
        return;
    }

    if tag == "img" {
        let src = element.attr("src").unwrap_or_default();
        let alt = element.attr("alt").unwrap_or_default();
        let width = element.attr("width").and_then(parse_dimension);
        let height = element.attr("height").and_then(parse_dimension);
        collector.push_image(src, alt, width, height, &style);
        collector.link = previous_link;
        return;
    }

    if tag == "svg" {
        if let Some(asset) = render_svg(&element.html()) {
            collector.images.push(asset);
            let index = collector.images.len() - 1;
            let width = element.attr("width").and_then(parse_dimension);
            let height = element.attr("height").and_then(parse_dimension);
            collector.push_image_item(index, width, height, &style);
        }
        collector.link = previous_link;
        return;
    }

    if tag == "table" {
        layout_table(&element, &style, rules, collector);
        collector.link = previous_link;
        return;
    }

    let starts_list = matches!(tag, "ul" | "ol");
    if starts_list {
        collector.lists.push(ListState {
            ordered: tag == "ol",
            counter: 0,
        });
    }

    let is_block = style.display == Display::Block;
    if is_block {
        collector.start_block(&style);
    }

    if tag == "li" {
        let prefix = if let Some(state) = collector.lists.last_mut() {
            state.counter += 1;
            if state.ordered {
                format!("{}. ", state.counter)
            } else {
                "• ".to_string()
            }
        } else {
            "• ".to_string()
        };
        collector.push_text(&prefix, &style);
    }

    for child in element.children() {
        if let Some(child_element) = ElementRef::wrap(child) {
            walk(child_element, &style, rules, collector);
        } else if let Some(text) = child.value().as_text() {
            collector.push_text(text, &style);
        }
    }

    if is_block {
        collector.end_block(&style);
    }

    if starts_list {
        collector.lists.pop();
    }
    collector.link = previous_link;
}

#[derive(Clone)]
struct TableCell<'a> {
    element: ElementRef<'a>,
    colspan: usize,
    rowspan: usize,
}

fn layout_table<'a>(
    table: &ElementRef<'a>,
    style: &Style,
    rules: &[Rule],
    collector: &mut Collector,
) {
    let row_selector = Selector::parse("tr").expect("valid selector");
    let rows = table.select(&row_selector).collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }

    let mut grid = Vec::<Vec<Option<TableCell>>>::new();
    let mut cursors = Vec::new();
    let mut column_count = 0usize;

    for row in &rows {
        let cells = row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|cell| matches!(cell.value().name(), "td" | "th"))
            .collect::<Vec<_>>();
        column_count = column_count.max(cells.len());
        grid.push(vec![None; column_count.max(1)]);
        cursors.push(0usize);
    }

    column_count = column_count.max(1);
    for row in &mut grid {
        row.resize(column_count, None);
    }

    for (row_index, row) in rows.iter().enumerate() {
        for cell in row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|cell| matches!(cell.value().name(), "td" | "th"))
        {
            while cursors[row_index] < column_count && grid[row_index][cursors[row_index]].is_some()
            {
                cursors[row_index] += 1;
            }
            if cursors[row_index] >= column_count {
                break;
            }

            let column = cursors[row_index];
            let colspan = parse_positive_attr(cell.attr("colspan")).unwrap_or(1);
            let rowspan = parse_positive_attr(cell.attr("rowspan")).unwrap_or(1);
            let item = TableCell {
                element: cell,
                colspan,
                rowspan,
            };

            let row_end = (row_index + rowspan).min(grid.len());
            let column_end = (column + colspan).min(column_count);
            for row in grid.iter_mut().take(row_end).skip(row_index) {
                for slot in row.iter_mut().take(column_end).skip(column) {
                    *slot = Some(item.clone());
                }
            }
            cursors[row_index] = column + colspan;
        }
    }

    let column_width = collector.max_width / column_count as f32;
    let table_space = style.margin_top_pt();

    for row_index in 0..grid.len() {
        let mut prepared = Vec::new();
        let mut visited = HashSet::new();
        let mut row_height: f32 = 0.0;

        for column_index in 0..column_count {
            let Some(cell) = grid[row_index][column_index].clone() else {
                continue;
            };
            if visited.contains(&(row_index, column_index)) {
                continue;
            }
            for target_row in row_index..(row_index + cell.rowspan).min(grid.len()) {
                for target_column in column_index..(column_index + cell.colspan).min(column_count) {
                    visited.insert((target_row, target_column));
                }
            }

            let cell_style = style_for(&cell.element, style, rules);
            let padding_left = cell_style.padding_left_pt();
            let padding_right = cell_style.padding_right_pt();
            let padding_top = cell_style.padding_top_pt();
            let padding_bottom = cell_style.padding_bottom_pt();
            let cell_width = column_width * cell.colspan as f32;
            let content_width = (cell_width - padding_left - padding_right).max(1.0);

            let saved_max = collector.max_width;
            let saved_page_height = collector.page_height;
            let saved_cursor = collector.cursor;
            let saved_align = collector.align;
            let saved_pending = collector.pending_before;
            let saved_background = collector.background;
            let saved_border = collector.border;
            let saved_stack_len = collector.style_stack.len();
            let saved_pages = collector.pages.len();

            collector.max_width = content_width;
            collector.page_height = f32::MAX;
            collector.cursor = 0.0;
            collector.new_page();
            collector.align = cell_style.text_align;
            collector.pending_before = 0.0;
            collector.background = None;
            collector.border = None;

            for child in cell.element.children() {
                if let Some(child_element) = ElementRef::wrap(child) {
                    walk(child_element, &cell_style, rules, collector);
                } else if let Some(text) = child.value().as_text() {
                    collector.push_text(text, &cell_style);
                }
            }
            collector.flush_line();

            let mut lines = collector.pages.pop().expect("cell page").lines;
            for line in &mut lines {
                line.background = None;
                line.border = None;
                line.background_width = None;
            }
            collector.pages.truncate(saved_pages);
            collector.style_stack.truncate(saved_stack_len);
            collector.max_width = saved_max;
            collector.page_height = saved_page_height;
            collector.cursor = saved_cursor;
            collector.align = saved_align;
            collector.pending_before = saved_pending;
            collector.background = saved_background;
            collector.border = saved_border;

            let content_height = lines
                .iter()
                .map(|line| line.space_before + line.height + line.space_after)
                .sum::<f32>();
            let height = (content_height + padding_top + padding_bottom).max(12.0);
            row_height = row_height.max(height);
            prepared.push((column_index, cell.colspan, cell_style, lines));
        }

        let space_before = if row_index == 0 { table_space } else { 0.0 };
        let row_top = collector.reserve(row_height, space_before);

        for (column_index, colspan, cell_style, lines) in prepared {
            let x = column_index as f32 * column_width;
            let cell_width = column_width * colspan as f32;
            let padding_left = cell_style.padding_left_pt();
            let padding_top = cell_style.padding_top_pt();

            let box_line = Line {
                items: Vec::new(),
                width: cell_width,
                height: row_height,
                space_before: 0.0,
                space_after: 0.0,
                top: row_top,
                left: x,
                align: TextAlign::Left,
                background: cell_style.background,
                background_width: Some(cell_width),
                border: cell_style.border,
                links: Vec::new(),
            };
            collector.push_line(box_line);

            for mut line in lines {
                line.top += row_top + padding_top;
                line.left = x + padding_left;
                collector.push_line(line);
            }
        }
    }
}

fn parse_positive_attr(value: Option<&str>) -> Option<usize> {
    value?.parse::<usize>().ok().filter(|value| *value > 0)
}

fn parse_dimension(value: &str) -> Option<f32> {
    let text = value.trim().to_ascii_lowercase();
    let (number, scale) = if let Some(number) = text.strip_suffix("px") {
        (number, 0.75)
    } else if let Some(number) = text.strip_suffix("pt") {
        (number, 1.0)
    } else {
        (text.as_str(), 1.0)
    };

    number.trim().parse::<f32>().ok().map(|value| value * scale)
}

pub fn measure(text: &str, size: f32, font: FontId, fonts: &[EmbeddedFont]) -> f32 {
    let units = text
        .chars()
        .map(|ch| match font {
            FontId::Builtin(_) => match ch as u32 {
                32..=126 => HELVETICA_WIDTHS[ch as usize - 32] as u32,
                _ => 556,
            },
            FontId::Embedded(index) => fonts
                .get(index)
                .and_then(|embedded| embedded.advance.get(&ch))
                .map(|advance| (advance * 1000.0).round() as u32)
                .unwrap_or(556),
        })
        .sum::<u32>();

    units as f32 / 1000.0 * size
}
