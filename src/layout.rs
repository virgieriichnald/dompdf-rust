use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use scraper::{ElementRef, Html, Selector};
use ttf_parser::Face;

use crate::css::{
    BackgroundRepeat, BackgroundSize, BorderCollapse, BorderSpec, Clear, Display, Float, FontFace,
    Length, ListStyle, MediaContext, PageRules, Position, PseudoElement, Rule, Style, TextAlign,
    VerticalAlign, WhiteSpace, parse_color, parse_font_faces, parse_length, parse_page_rules,
    parse_styles, pseudo_style_for, px_scale, style_for,
};
use crate::{Options, PageTextLocation, ResourcePolicy, read_bytes};

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
    pub alpha: f32,
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    pub link: Option<String>,
    pub spacing: f32,
    pub width: f32,
}

#[derive(Clone, Debug)]
pub struct ImageItem {
    pub image: usize,
    pub width: f32,
    pub height: f32,
    pub link: Option<String>,
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
pub struct Anchor {
    pub name: String,
    pub page: usize,
    pub top: f32,
}

#[derive(Clone, Debug)]
pub struct Bookmark {
    pub title: String,
    pub page: usize,
    pub top: f32,
    pub level: usize,
}

#[derive(Clone, Debug)]
struct BlockStyle {
    background: Option<[f32; 3]>,
    background_alpha: f32,
    background_image: Option<usize>,
    background_repeat: BackgroundRepeat,
    background_size: BackgroundSize,
    background_position_x: f32,
    background_position_y: f32,
    border: Option<BorderSpec>,
    opacity: f32,
    border_radius: [f32; 4],
}

impl Default for BlockStyle {
    fn default() -> Self {
        Self {
            background: None,
            background_alpha: 1.0,
            background_image: None,
            background_repeat: BackgroundRepeat::Repeat,
            background_size: BackgroundSize::Auto,
            background_position_x: 0.0,
            background_position_y: 0.0,
            border: None,
            opacity: 1.0,
            border_radius: [0.0; 4],
        }
    }
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
    pub background_alpha: f32,
    pub background_image: Option<usize>,
    pub background_repeat: BackgroundRepeat,
    pub background_size: BackgroundSize,
    pub background_position_x: f32,
    pub background_position_y: f32,
    pub background_width: Option<f32>,
    pub border: Option<BorderSpec>,
    pub opacity: f32,
    pub border_radius: [f32; 4],
    pub links: Vec<LinkRect>,
}

#[derive(Clone, Copy, Debug)]
pub struct PageGeometry {
    pub width: f32,
    pub height: f32,
    pub margin: f32,
}

#[derive(Clone, Debug)]
pub struct Overlay {
    pub line: Line,
    pub z: i32,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub lines: Vec<Line>,
    pub overlays: Vec<Overlay>,
    pub geometry: PageGeometry,
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
    pub bold: bool,
    pub italic: bool,
    pub data: Vec<u8>,
    pub advance: HashMap<char, f32>,
    pub glyphs: HashMap<char, u16>,
    pub units_per_em: u16,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub pages: Vec<Page>,
    pub anchors: Vec<Anchor>,
    pub bookmarks: Vec<Bookmark>,
    pub images: Vec<ImageAsset>,
    pub fonts: Vec<EmbeddedFont>,
    pub metadata: Metadata,
    pub width: f32,
    pub height: f32,
    pub margin: f32,
    pub page_text: Option<String>,
    pub page_text_location: PageTextLocation,
    pub px_scale: f32,
}

impl Document {
    pub fn from_html(html: &str) -> Self {
        Self::from_html_with_base(html, Path::new("."))
    }

    pub fn from_html_with_base(html: &str, base: &Path) -> Self {
        Self::from_html_with_options(html, base, &Options::default())
    }

    pub fn from_html_with_options(html: &str, base: &Path, options: &Options) -> Self {
        let parsed = Html::parse_document(html);
        let base = resolve_base(&parsed, base);
        let metadata = parse_metadata(&parsed);
        let mut options = options.clone();
        let provisional_media = MediaContext {
            media_type: "print".to_string(),
            width: options.width * options.dpi / 72.0,
            height: options.height * options.dpi / 72.0,
        };
        let page_rules = parse_page_rules(
            &parsed,
            &base,
            options.dpi,
            &provisional_media,
            &options.resource,
        );
        let default_page = page_rules.default.clone();
        if let Some(width) = default_page.width {
            options.width = width;
        }
        if let Some(height) = default_page.height {
            options.height = height;
        }
        if let Some(margin) = default_page.margin {
            options.margin = margin;
        }
        let media = MediaContext {
            media_type: "print".to_string(),
            width: options.width * options.dpi / 72.0,
            height: options.height * options.dpi / 72.0,
        };
        let rules = parse_styles(&parsed, &base, &media, &options.resource);
        let font_faces = parse_font_faces(&parsed, &base, &media, &options.resource);
        let base_style = Style {
            font_family: options.default_font.clone(),
            px_scale: px_scale(options.dpi),
            containing_width: (options.width - 2.0 * options.margin).max(1.0),
            ..Style::default()
        };
        let geometry = PageGeometry {
            width: options.width,
            height: options.height,
            margin: options.margin,
        };
        let mut collector = Collector::new(
            geometry,
            &base,
            base_style.px_scale,
            options.image_byte_limit,
            options.resource.clone(),
            page_rules,
        );
        let initial_geometry = collector.geometry_for(0);
        collector.pages[0].geometry = initial_geometry;
        collector.apply_geometry(initial_geometry);

        collector.load_fonts(&font_faces);
        walk(parsed.root_element(), &base_style, &rules, &mut collector);
        collector.flush_line();

        Document {
            pages: collector.pages,
            anchors: collector.anchors,
            bookmarks: collector.bookmarks,
            images: collector.images,
            fonts: collector.fonts,
            metadata,
            width: options.width,
            height: options.height,
            margin: options.margin,
            page_text: options.page_text.clone(),
            page_text_location: options.page_text_location,
            px_scale: base_style.px_scale,
        }
    }
}

fn resolve_base(html: &Html, base: &Path) -> PathBuf {
    let selector = Selector::parse("base[href]").expect("valid selector");
    let Some(element) = html.select(&selector).next() else {
        return base.to_path_buf();
    };
    let Some(href) = element.value().attr("href") else {
        return base.to_path_buf();
    };
    if href.starts_with("data:")
        || href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("//")
    {
        return base.to_path_buf();
    }

    let joined = base.join(href);
    if href.ends_with('/') {
        joined
    } else {
        joined
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| base.to_path_buf())
    }
}

fn parse_metadata(html: &Html) -> Metadata {
    let title_selector = Selector::parse("title").expect("valid selector");
    let meta_selector = Selector::parse("meta[name]").expect("valid selector");
    let mut metadata = Metadata::default();

    if let Some(element) = html.select(&title_selector).next() {
        let title = element.text().collect::<String>().trim().to_string();
        if !title.is_empty() {
            metadata.title = Some(title);
        }
    }

    for element in html.select(&meta_selector) {
        let value = element.value();
        let Some(name) = value.attr("name") else {
            continue;
        };
        let Some(content) = value.attr("content") else {
            continue;
        };
        let content = content.trim();
        if content.is_empty() {
            continue;
        }

        match name.to_ascii_lowercase().as_str() {
            "author" => metadata.author = Some(content.to_string()),
            "description" => metadata.subject = Some(content.to_string()),
            "keywords" => metadata.keywords = Some(content.to_string()),
            _ => {}
        }
    }

    metadata
}

fn resolve_uri(href: &str, base: &Path) -> String {
    if href.starts_with('#')
        || href.starts_with("data:")
        || href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with("//")
    {
        return href.to_string();
    }
    base.join(href).to_string_lossy().into_owned()
}

#[derive(Clone, Copy, Debug)]
struct ListState {
    style: ListStyle,
    counter: i64,
    step: i64,
}

struct Collector {
    pages: Vec<Page>,
    anchors: Vec<Anchor>,
    bookmarks: Vec<Bookmark>,
    current: Vec<InlineItem>,
    width: f32,
    height: f32,
    default_geometry: PageGeometry,
    page_rules: PageRules,
    page_name: Option<String>,
    pending_before: f32,
    pending_indent: f32,
    offset_x: f32,
    offset_y: f32,
    offset_stack: Vec<(f32, f32)>,
    float_left_bottom: f32,
    float_right_bottom: f32,
    align: TextAlign,
    max_width: f32,
    page_height: f32,
    cursor: f32,
    base: PathBuf,
    images: Vec<ImageAsset>,
    image_cache: HashMap<String, Option<usize>>,
    fonts: Vec<EmbeddedFont>,
    font_cache: HashMap<String, Option<usize>>,
    block: BlockStyle,
    style_stack: Vec<BlockStyle>,
    link: Option<String>,
    lists: Vec<ListState>,
    counters: HashMap<String, Vec<i32>>,
    reset_stack: Vec<Vec<String>>,
    px_scale: f32,
    image_byte_limit: Option<usize>,
    policy: ResourcePolicy,
}

impl Collector {
    fn new(
        geometry: PageGeometry,
        base: &Path,
        px_scale: f32,
        image_byte_limit: Option<usize>,
        policy: ResourcePolicy,
        page_rules: PageRules,
    ) -> Self {
        let content_width = geometry.width - 2.0 * geometry.margin;
        let content_height = geometry.height - 2.0 * geometry.margin;
        Self {
            pages: vec![Page {
                lines: Vec::new(),
                overlays: Vec::new(),
                geometry,
            }],
            anchors: Vec::new(),
            bookmarks: Vec::new(),
            current: Vec::new(),
            width: 0.0,
            height: 0.0,
            default_geometry: geometry,
            page_rules,
            page_name: None,
            pending_before: 0.0,
            pending_indent: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            offset_stack: Vec::new(),
            float_left_bottom: 0.0,
            float_right_bottom: 0.0,
            align: TextAlign::Left,
            max_width: content_width,
            page_height: content_height,
            cursor: 0.0,
            base: base.to_path_buf(),
            images: Vec::new(),
            image_cache: HashMap::new(),
            fonts: Vec::new(),
            font_cache: HashMap::new(),
            block: BlockStyle::default(),
            style_stack: Vec::new(),
            link: None,
            lists: Vec::new(),
            counters: HashMap::new(),
            reset_stack: Vec::new(),
            px_scale,
            image_byte_limit,
            policy,
        }
    }

    fn geometry_for(&self, index: usize) -> PageGeometry {
        let rule = self.page_rules.for_index(index, self.page_name.as_deref());
        PageGeometry {
            width: rule.width.unwrap_or(self.default_geometry.width),
            height: rule.height.unwrap_or(self.default_geometry.height),
            margin: rule.margin_points(self.default_geometry.margin),
        }
    }

    fn apply_geometry(&mut self, geometry: PageGeometry) {
        self.max_width = geometry.width - 2.0 * geometry.margin;
        self.page_height = geometry.height - 2.0 * geometry.margin;
    }

    fn new_page(&mut self) {
        let index = self.pages.len();
        let geometry = self.geometry_for(index);
        self.pages.push(Page {
            lines: Vec::new(),
            overlays: Vec::new(),
            geometry,
        });
        self.apply_geometry(geometry);
        self.cursor = 0.0;
        self.float_left_bottom = 0.0;
        self.float_right_bottom = 0.0;
    }

    fn page_is_empty(&self) -> bool {
        self.pages.last().is_some_and(|page| page.lines.is_empty())
    }

    fn push_line(&mut self, mut line: Line) {
        if line.background.is_none() {
            line.background = self.block.background;
            line.background_alpha = self.block.background_alpha;
        }
        if line.background_image.is_none() {
            line.background_image = self.block.background_image;
            line.background_repeat = self.block.background_repeat;
            line.background_size = self.block.background_size;
            line.background_position_x = self.block.background_position_x;
            line.background_position_y = self.block.background_position_y;
        }
        if line.border.is_none() {
            line.border = self.block.border;
        }
        line.opacity *= self.block.opacity;
        if line.border_radius.iter().all(|radius| *radius == 0.0) {
            line.border_radius = self.block.border_radius;
        }
        if line.background_width.is_none()
            && (self.block.background.is_some() || self.block.background_image.is_some())
        {
            line.background_width = Some(self.max_width);
        }
        if line.links.is_empty() {
            line.links = collect_links(&line.items);
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
        line.top = self.cursor + self.offset_y;
        line.left = self.pending_indent + self.offset_x;
        self.pending_indent = 0.0;
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
            background_alpha: 1.0,
            background_image: None,
            background_repeat: BackgroundRepeat::Repeat,
            background_size: BackgroundSize::Auto,
            background_position_x: 0.0,
            background_position_y: 0.0,
            background_width: None,
            border: None,
            opacity: 1.0,
            border_radius: [0.0; 4],
            links: Vec::new(),
        };

        self.width = 0.0;
        self.height = 0.0;
        self.pending_before = 0.0;
        self.emit_line(line);
    }

    fn start_block(&mut self, style: &Style) {
        self.flush_line();
        self.style_stack.push(self.block.clone());
        let mut reset_names = Vec::new();
        if let Some((name, value)) = &style.counter_reset {
            self.counters.entry(name.clone()).or_default().push(*value);
            reset_names.push(name.clone());
        }
        increment_counter(&mut self.counters, style);
        self.reset_stack.push(reset_names);
        self.offset_stack.push((self.offset_x, self.offset_y));
        if style.position == Position::Relative {
            let dx = if let Some(left) = style.offset_left {
                left.resolve_with(style.font_size, style.px_scale)
            } else {
                style
                    .offset_right
                    .map(|right| -right.resolve_with(style.font_size, style.px_scale))
                    .unwrap_or(0.0)
            };
            let dy = if let Some(top) = style.offset_top {
                top.resolve_with(style.font_size, style.px_scale)
            } else {
                style
                    .offset_bottom
                    .map(|bottom| -bottom.resolve_with(style.font_size, style.px_scale))
                    .unwrap_or(0.0)
            };
            self.offset_x += dx;
            self.offset_y += dy;
        }
        if style.background.is_some() {
            self.block.background = style.background;
            self.block.background_alpha = style.background_alpha;
        }
        if let Some(url) = &style.background_image {
            self.block.background_image = self.load_image_index(url);
        }
        self.block.background_repeat = style.background_repeat;
        self.block.background_size = style.background_size;
        self.block.background_position_x = style.background_position_x;
        self.block.background_position_y = style.background_position_y;
        if style.border.is_some() {
            self.block.border = style.border;
        }
        self.block.opacity *= style.opacity;
        if style.border_radius.iter().any(|radius| *radius > 0.0) {
            self.block.border_radius = style.border_radius;
        }
        self.pending_before = self.pending_before.max(style.margin_top_pt());
        self.pending_indent = style.text_indent_pt();
        self.align = style.text_align;
    }

    fn end_block(&mut self, style: &Style) {
        self.flush_line();
        let margin = style.margin_bottom_pt();
        if let Some(last) = self.pages.last_mut().and_then(|page| page.lines.last_mut()) {
            last.space_after = last.space_after.max(margin);
        }
        self.cursor += margin;
        if let Some(block) = self.style_stack.pop() {
            self.block = block;
        }
        if let Some(names) = self.reset_stack.pop() {
            for name in names {
                if let Some(values) = self.counters.get_mut(&name) {
                    values.pop();
                    if values.is_empty() {
                        self.counters.remove(&name);
                    }
                }
            }
        }
        if let Some((x, y)) = self.offset_stack.pop() {
            self.offset_x = x;
            self.offset_y = y;
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

        let kind = FontKind::from_style(style);
        let bold = matches!(kind, FontKind::Bold | FontKind::BoldOblique);
        let italic = matches!(kind, FontKind::Oblique | FontKind::BoldOblique);

        self.fonts
            .iter()
            .position(|font| {
                font.family.eq_ignore_ascii_case(&family)
                    && font.bold == bold
                    && font.italic == italic
            })
            .or_else(|| {
                self.fonts
                    .iter()
                    .position(|font| font.family.eq_ignore_ascii_case(&family))
            })
            .map(FontId::Embedded)
            .unwrap_or(FontId::Builtin(kind))
    }

    fn text_width(&self, text: &str, style: &Style) -> f32 {
        let spacing = style.letter_spacing_pt();
        measure(text, style.font_size, self.font_id(style), &self.fonts)
            + spacing * text.chars().count() as f32
    }

    fn push_text(&mut self, text: &str, style: &Style) {
        if style.white_space == WhiteSpace::Pre {
            self.push_text_pre(text, style);
            return;
        }

        let space_width = self.text_width(" ", style);
        let wrap = style.white_space != WhiteSpace::Nowrap;

        for word in text.split_whitespace() {
            let word_width = self.text_width(word, style);
            let extra = if self.current.is_empty() {
                0.0
            } else {
                space_width
            };

            if wrap && !self.current.is_empty() && self.width + extra + word_width > self.max_width
            {
                self.flush_line();
            }

            if !self.current.is_empty() {
                self.push_run(" ", style, space_width);
            }

            self.push_run(word, style, word_width);
            self.height = self.height.max(style.font_size * style.line_height);
        }
    }

    fn push_text_pre(&mut self, text: &str, style: &Style) {
        let space_width = self.text_width(" ", style);
        let mut word = String::new();

        for ch in text.chars() {
            if ch == '\n' {
                self.push_pre_word(&word, style);
                word.clear();
                self.flush_line();
                continue;
            }
            if ch.is_whitespace() {
                self.push_pre_word(&word, style);
                word.clear();
                self.push_run(" ", style, space_width);
            } else {
                word.push(ch);
            }
        }
        self.push_pre_word(&word, style);
        self.height = self.height.max(style.font_size * style.line_height);
    }

    fn push_pre_word(&mut self, word: &str, style: &Style) {
        if !word.is_empty() {
            let width = self.text_width(word, style);
            self.push_run(word, style, width);
        }
    }

    fn push_run(&mut self, text: &str, style: &Style, width: f32) {
        self.current.push(InlineItem::Text(Run {
            text: text.to_string(),
            font: self.font_id(style),
            size: style.font_size,
            color: style.color,
            alpha: style.color_alpha,
            underline: style.underline,
            overline: style.overline,
            line_through: style.line_through,
            link: self.link.clone(),
            spacing: style.letter_spacing_pt(),
            width,
        }));
        self.width += width;
    }

    fn load_image_index(&mut self, src: &str) -> Option<usize> {
        let key = src.to_string();
        if let Some(cached) = self.image_cache.get(&key) {
            return *cached;
        }

        let loaded = read_bytes(src, &self.base, &self.policy)
            .and_then(|bytes| {
                if self
                    .image_byte_limit
                    .is_some_and(|limit| bytes.len() > limit)
                {
                    return None;
                }
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
    }

    fn push_image(
        &mut self,
        src: &str,
        alt: &str,
        attr_width: Option<f32>,
        attr_height: Option<f32>,
        style: &Style,
    ) {
        let Some(index) = self.load_image_index(src) else {
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
        let natural_width = asset.width as f32 * self.px_scale;
        let natural_height = asset.height as f32 * self.px_scale;
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
            link: self.link.clone(),
        }));
        self.width += width;
        self.height = self
            .height
            .max(height)
            .max(style.font_size * style.line_height);
    }

    fn load_fonts(&mut self, faces: &[FontFace]) {
        for face in faces {
            let key = format!(
                "{}|{}|{}",
                face.family.to_ascii_lowercase(),
                face.bold,
                face.italic
            );
            if !self.font_cache.contains_key(&key) {
                let loaded = read_bytes(&face.src, &face.base, &self.policy)
                    .and_then(|bytes| load_font(&face.family, &bytes, face.bold, face.italic))
                    .map(|font| {
                        self.fonts.push(font);
                        self.fonts.len() - 1
                    });
                self.font_cache.insert(key, loaded);
            }
        }
    }
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

fn load_font(family: &str, bytes: &[u8], bold: bool, italic: bool) -> Option<EmbeddedFont> {
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
        bold,
        italic,
        data: bytes.to_vec(),
        advance,
        glyphs,
        units_per_em,
        ascent: face.ascender() as f32 / units_per_em as f32,
        descent: face.descender() as f32 / units_per_em as f32,
    })
}

fn collect_links(items: &[InlineItem]) -> Vec<LinkRect> {
    let mut links = Vec::new();
    let mut cursor = 0.0;
    let mut active: Option<(String, f32, f32)> = None;

    for item in items {
        let (width, link) = match item {
            InlineItem::Text(run) => (run.width, run.link.as_ref()),
            InlineItem::Image(image) => (image.width, image.link.as_ref()),
        };

        match (link, active.as_mut()) {
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

fn list_marker(style: ListStyle, value: i64) -> Option<String> {
    let marker = match style {
        ListStyle::Disc => "• ".to_string(),
        ListStyle::Circle => "○ ".to_string(),
        ListStyle::Square => "▪ ".to_string(),
        ListStyle::None => return None,
        ListStyle::Decimal => format!("{value}. "),
        ListStyle::LowerAlpha => format!("{}. ", dec2base26(value, true)),
        ListStyle::UpperAlpha => format!("{}. ", dec2base26(value, false)),
        ListStyle::LowerRoman => format!("{}. ", dec2roman(value).to_ascii_lowercase()),
        ListStyle::UpperRoman => format!("{}. ", dec2roman(value)),
    };

    Some(marker)
}

fn dec2base26(value: i64, lowercase: bool) -> String {
    let mut letters = Vec::new();
    let mut number = value.max(1) as u64;
    while number > 0 {
        let digit = ((number - 1) % 26) as u8;
        letters.push(if lowercase {
            b'a' + digit
        } else {
            b'A' + digit
        });
        number = (number - 1) / 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

fn dec2roman(value: i64) -> String {
    const NUMERALS: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut remaining = value.max(1);
    let mut roman = String::new();
    for (amount, numeral) in NUMERALS {
        while remaining >= amount {
            roman.push_str(numeral);
            remaining -= amount;
        }
    }

    roman
}

fn increment_counter(counters: &mut HashMap<String, Vec<i32>>, style: &Style) {
    if let Some((name, delta)) = &style.counter_increment {
        let values = counters.entry(name.clone()).or_insert_with(|| vec![0]);
        if let Some(last) = values.last_mut() {
            *last += delta;
        } else {
            values.push(*delta);
        }
    }
}

fn apply_counter_style(counters: &mut HashMap<String, Vec<i32>>, style: &Style) {
    if let Some((name, value)) = &style.counter_reset {
        if let Some(values) = counters.get_mut(name) {
            values.push(*value);
        } else {
            counters.insert(name.clone(), vec![*value]);
        }
    }
    increment_counter(counters, style);
}

fn format_counter(counters: &HashMap<String, Vec<i32>>, name: &str) -> String {
    counters
        .get(name)
        .and_then(|values| values.last())
        .copied()
        .unwrap_or(0)
        .to_string()
}

fn format_counters(counters: &HashMap<String, Vec<i32>>, name: &str, separator: &str) -> String {
    counters
        .get(name)
        .map(|values| {
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(separator)
        })
        .unwrap_or_else(|| "0".to_string())
}

fn resolve_generated_content(
    input: &str,
    element: &ElementRef,
    counters: &HashMap<String, Vec<i32>>,
) -> Option<String> {
    let mut output = String::new();
    let mut rest = input.trim();

    while !rest.is_empty() {
        let quote = rest.chars().next()?;
        if let Some(after) = rest.strip_prefix("counters(") {
            let Some(end) = after.find(')') else {
                break;
            };
            let mut args = after[..end].split(',');
            let name = args.next().unwrap_or("").trim().trim_matches(['"', '\'']);
            let separator = args.next().unwrap_or(".").trim().trim_matches(['"', '\'']);
            output.push_str(&format_counters(counters, name, separator));
            rest = after[end + 1..].trim_start();
        } else if let Some(after) = rest.strip_prefix("counter(") {
            let Some(end) = after.find(')') else {
                break;
            };
            let name = after[..end].trim().trim_matches(['"', '\'']);
            output.push_str(&format_counter(counters, name));
            rest = after[end + 1..].trim_start();
        } else if quote == '"' || quote == '\'' {
            let Some(end) = rest[1..].find(quote) else {
                break;
            };
            output.push_str(&rest[1..end + 1]);
            rest = rest[end + 2..].trim_start();
        } else if let Some(after) = rest.strip_prefix("attr(") {
            let Some(end) = after.find(')') else {
                break;
            };
            let name = after[..end].trim().trim_matches(['"', '\'']);
            if let Some(value) = element.attr(name) {
                output.push_str(value);
            }
            rest = after[end + 1..].trim_start();
        } else if rest.starts_with("open-quote") {
            output.push('“');
            rest = rest["open-quote".len()..].trim_start();
        } else if rest.starts_with("close-quote") {
            output.push('”');
            rest = rest["close-quote".len()..].trim_start();
        } else {
            output.push(quote);
            rest = rest[quote.len_utf8()..].trim_start();
        }
    }

    (!output.is_empty()).then_some(output)
}

fn heading_level(tag: &str) -> Option<usize> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn collect_element_text(element: ElementRef, output: &mut String) {
    for child in element.children() {
        if let Some(child_element) = ElementRef::wrap(child) {
            collect_element_text(child_element, output);
        } else if let Some(text) = child.value().as_text() {
            output.push_str(text);
        }
    }
}

fn normalize_title(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

    if collector.page_name.is_none()
        && let Some(name) = &style.page_name
    {
        collector.page_name = Some(name.clone());
        let geometry = collector.geometry_for(0);
        collector.pages[0].geometry = geometry;
        collector.apply_geometry(geometry);
    }

    if tag == "table"
        && let Some(width) = element
            .attr("border")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| *value > 0.0)
    {
        style.border = Some(BorderSpec {
            width,
            color: [0.0, 0.0, 0.0],
            alpha: 1.0,
            top: true,
            right: true,
            bottom: true,
            left: true,
        });
    }

    if let Some(name) = element
        .attr("id")
        .or_else(|| element.attr("name"))
        .filter(|name| !name.is_empty())
        && !collector.anchors.iter().any(|anchor| anchor.name == name)
    {
        collector.anchors.push(Anchor {
            name: name.to_string(),
            page: collector.pages.len() - 1,
            top: collector.cursor,
        });
    }

    if style.display == Display::None {
        return;
    }

    let previous_link = collector.link.clone();
    if tag == "a" {
        collector.link = element
            .attr("href")
            .map(|href| resolve_uri(href, &collector.base));
    }

    if style.clear != Clear::None {
        collector.flush_line();
        let bottom = match style.clear {
            Clear::Left => collector.float_left_bottom,
            Clear::Right => collector.float_right_bottom,
            Clear::Both => collector
                .float_left_bottom
                .max(collector.float_right_bottom),
            Clear::None => 0.0,
        };
        collector.cursor = collector.cursor.max(bottom);
    }

    if style.float != Float::None {
        layout_float(&element, &style, rules, collector);
        collector.link = previous_link;
        return;
    }

    if matches!(style.position, Position::Absolute | Position::Fixed) {
        layout_overlay(&element, &style, rules, collector, None);
        collector.link = previous_link;
        return;
    }

    if let Some(level) = heading_level(tag) {
        let mut text = String::new();
        collect_element_text(element, &mut text);
        let title = normalize_title(&text);
        if !title.is_empty() {
            collector.bookmarks.push(Bookmark {
                title,
                page: collector.pages.len() - 1,
                top: collector.cursor,
                level,
            });
        }
    }

    if tag == "br" {
        collector.flush_line();
        collector.link = previous_link;
        return;
    }

    if tag == "img" {
        let src = element.attr("src").unwrap_or_default();
        let alt = element.attr("alt").unwrap_or_default();
        let width = element
            .attr("width")
            .and_then(|value| parse_dimension(value, collector.px_scale));
        let height = element
            .attr("height")
            .and_then(|value| parse_dimension(value, collector.px_scale));
        collector.push_image(src, alt, width, height, &style);
        collector.link = previous_link;
        return;
    }

    if tag == "svg" {
        if let Some(asset) = render_svg(&element.html()) {
            collector.images.push(asset);
            let index = collector.images.len() - 1;
            let width = element
                .attr("width")
                .and_then(|value| parse_dimension(value, collector.px_scale));
            let height = element
                .attr("height")
                .and_then(|value| parse_dimension(value, collector.px_scale));
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
        let start = element
            .attr("start")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(1);
        let step = if element.attr("reversed").is_some() {
            -1
        } else {
            1
        };
        collector.lists.push(ListState {
            style: style.list_style,
            counter: start,
            step,
        });
    }

    let is_block = style.display == Display::Block;
    if is_block {
        collector.start_block(&style);
    }

    if tag == "li" {
        let prefix = collector
            .lists
            .last_mut()
            .and_then(|state| {
                let value = state.counter;
                state.counter += state.step;
                list_marker(state.style, value)
            })
            .unwrap_or_else(|| "• ".to_string());
        collector.push_text(&prefix, &style);
    }

    let before_style = pseudo_style_for(&element, &style, rules, PseudoElement::Before);
    let after_style = pseudo_style_for(&element, &style, rules, PseudoElement::After);
    let mut before_counters = collector.counters.clone();
    apply_counter_style(&mut before_counters, &before_style);
    increment_counter(&mut collector.counters, &before_style);
    let mut after_counters = collector.counters.clone();
    apply_counter_style(&mut after_counters, &after_style);
    increment_counter(&mut collector.counters, &after_style);
    let before_content = before_style
        .content
        .as_deref()
        .and_then(|content| resolve_generated_content(content, &element, &before_counters));
    let after_content = after_style
        .content
        .as_deref()
        .and_then(|content| resolve_generated_content(content, &element, &after_counters));

    if let Some(content) = &before_content {
        collector.push_text(content, &before_style);
    }

    for child in element.children() {
        if let Some(child_element) = ElementRef::wrap(child) {
            walk(child_element, &style, rules, collector);
        } else if let Some(text) = child.value().as_text() {
            collector.push_text(text, &style);
        }
    }

    if let Some(content) = &after_content {
        collector.push_text(content, &after_style);
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

#[derive(Clone)]
struct PreparedCell {
    column: usize,
    colspan: usize,
    style: Style,
    lines: Vec<Line>,
}

#[derive(Clone)]
struct PreparedRow {
    height: f32,
    cells: Vec<PreparedCell>,
}

fn prepare_cell_style(
    cell: &TableCell<'_>,
    style: &Style,
    rules: &[Rule],
    padding: Option<Length>,
) -> Style {
    let mut cell_style = style_for(&cell.element, style, rules);
    let owns_padding = cell
        .element
        .attr("style")
        .is_some_and(|value| value.to_ascii_lowercase().contains("padding"));
    if !owns_padding && let Some(padding) = padding {
        cell_style.padding_top = padding;
        cell_style.padding_right = padding;
        cell_style.padding_bottom = padding;
        cell_style.padding_left = padding;
    }
    cell_style
}

fn column_offset(widths: &[f32], spacing: f32, column: usize) -> f32 {
    widths[..column].iter().copied().sum::<f32>() + spacing * column as f32
}

fn cell_span_width(widths: &[f32], spacing: f32, column: usize, colspan: usize) -> f32 {
    widths[column..(column + colspan).min(widths.len())]
        .iter()
        .copied()
        .sum::<f32>()
        + spacing * colspan.saturating_sub(1) as f32
}

fn emit_prepared_row(
    collector: &mut Collector,
    row: &PreparedRow,
    top: f32,
    widths: &[f32],
    spacing: f32,
) {
    for cell in &row.cells {
        let x = column_offset(widths, spacing, cell.column);
        let cell_width = cell_span_width(widths, spacing, cell.column, cell.colspan);
        let padding_left = cell.style.padding_left_pt();
        let padding_top = cell.style.padding_top_pt();
        let content_height = cell
            .lines
            .iter()
            .map(|line| line.space_before + line.height + line.space_after)
            .sum::<f32>();
        let free = (row.height - content_height).max(0.0);
        let content_offset = match cell.style.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Middle => free * 0.5,
            VerticalAlign::Bottom => free,
            VerticalAlign::Baseline => 0.0,
        };

        let box_line = Line {
            items: Vec::new(),
            width: cell_width,
            height: row.height,
            space_before: 0.0,
            space_after: 0.0,
            top,
            left: x,
            align: TextAlign::Left,
            background: cell.style.background,
            background_alpha: cell.style.background_alpha,
            background_image: cell
                .style
                .background_image
                .as_deref()
                .and_then(|url| collector.load_image_index(url)),
            background_repeat: cell.style.background_repeat,
            background_size: cell.style.background_size,
            background_position_x: cell.style.background_position_x,
            background_position_y: cell.style.background_position_y,
            background_width: Some(cell_width),
            border: cell.style.border,
            opacity: cell.style.opacity,
            border_radius: cell.style.border_radius,
            links: Vec::new(),
        };
        collector.push_line(box_line);

        for line in &cell.lines {
            let mut line = line.clone();
            line.top += top + padding_top + content_offset;
            line.left += x + padding_left;
            collector.push_line(line);
        }
    }
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

    let table_background = table.attr("bgcolor").and_then(parse_color);
    let cell_padding = table.attr("cellpadding").and_then(parse_length);
    let spacing = if style.border_collapse == BorderCollapse::Collapse {
        0.0
    } else {
        style.border_spacing.max(0.0)
    };

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
                colspan: colspan.min(column_count - column),
                rowspan,
            };

            let row_end = (row_index + rowspan).min(grid.len());
            let column_end = (column + item.colspan).min(column_count);
            for row in grid.iter_mut().take(row_end).skip(row_index) {
                for slot in row.iter_mut().take(column_end).skip(column) {
                    *slot = Some(item.clone());
                }
            }
            cursors[row_index] = column + item.colspan;
        }
    }

    let mut desired = vec![0.0f32; column_count];
    let mut measured = HashSet::new();
    for (row_index, row) in grid.iter().enumerate() {
        for (column_index, cell) in row.iter().enumerate() {
            let Some(cell) = cell else { continue };
            if !measured.insert((row_index, column_index)) {
                continue;
            }
            let cell_style = prepare_cell_style(cell, style, rules, cell_padding);
            let text = cell.element.text().collect::<String>();
            let natural = cell
                .element
                .attr("width")
                .and_then(parse_length)
                .map(|length| length.resolve_with(cell_style.font_size, cell_style.px_scale))
                .unwrap_or_else(|| collector.text_width(text.trim(), &cell_style));
            let padding = cell_style.padding_left_pt() + cell_style.padding_right_pt();
            let needed = (natural + padding) / cell.colspan as f32;
            for column in desired
                .iter_mut()
                .take((column_index + cell.colspan).min(column_count))
                .skip(column_index)
            {
                *column = (*column).max(needed);
            }
        }
    }

    let available =
        (collector.max_width - spacing * column_count.saturating_sub(1) as f32).max(1.0);
    let desired_total: f32 = desired.iter().sum();
    let column_widths = if desired_total <= 0.0 {
        vec![available / column_count as f32; column_count]
    } else {
        desired
            .iter()
            .map(|width| (width / desired_total * available).max(1.0))
            .collect::<Vec<_>>()
    };
    let table_space = style.margin_top_pt();
    let header_ptrs = table
        .select(&Selector::parse("thead tr").expect("valid selector"))
        .map(|row| row.value() as *const _ as usize)
        .collect::<HashSet<_>>();
    let mut header_row: Option<PreparedRow> = None;

    for row_index in 0..grid.len() {
        let mut prepared = PreparedRow {
            height: 0.0,
            cells: Vec::new(),
        };
        let mut visited = HashSet::new();

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

            let mut cell_style = prepare_cell_style(&cell, style, rules, cell_padding);
            if cell_style.background.is_none() {
                cell_style.background = table_background;
            }
            let padding_left = cell_style.padding_left_pt();
            let padding_right = cell_style.padding_right_pt();
            let padding_top = cell_style.padding_top_pt();
            let padding_bottom = cell_style.padding_bottom_pt();
            let cell_width = cell_span_width(&column_widths, spacing, column_index, cell.colspan);
            let content_width = (cell_width - padding_left - padding_right).max(1.0);

            let saved_max = collector.max_width;
            let saved_page_height = collector.page_height;
            let saved_cursor = collector.cursor;
            let saved_align = collector.align;
            let saved_pending = collector.pending_before;
            let saved_block = collector.block.clone();
            let saved_stack_len = collector.style_stack.len();
            let saved_pages = collector.pages.len();
            let saved_anchors = collector.anchors.len();
            let saved_bookmarks = collector.bookmarks.len();

            collector.max_width = content_width;
            collector.page_height = f32::MAX;
            collector.cursor = 0.0;
            collector.new_page();
            collector.align = cell_style.text_align;
            collector.pending_before = 0.0;
            collector.block = BlockStyle::default();

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
                line.background_image = None;
                line.border = None;
                line.background_width = None;
                line.opacity *= cell_style.opacity;
                line.border_radius = cell_style.border_radius;
            }
            collector.pages.truncate(saved_pages);
            collector.anchors.truncate(saved_anchors);
            collector.bookmarks.truncate(saved_bookmarks);
            collector.style_stack.truncate(saved_stack_len);
            collector.max_width = saved_max;
            collector.page_height = saved_page_height;
            collector.cursor = saved_cursor;
            collector.align = saved_align;
            collector.pending_before = saved_pending;
            collector.block = saved_block;

            let content_height = lines
                .iter()
                .map(|line| line.space_before + line.height + line.space_after)
                .sum::<f32>();
            prepared.height = prepared
                .height
                .max(content_height + padding_top + padding_bottom)
                .max(12.0);
            prepared.cells.push(PreparedCell {
                column: column_index,
                colspan: cell.colspan,
                style: cell_style,
                lines,
            });
        }

        let is_header = header_ptrs.contains(&(rows[row_index].value() as *const _ as usize));
        let space_before = if row_index == 0 { table_space } else { 0.0 };
        if let Some(header) = &header_row
            && !is_header
            && !collector.page_is_empty()
            && collector.cursor + space_before + prepared.height + header.height
                > collector.page_height
        {
            let header_top = collector.reserve(header.height, 0.0);
            emit_prepared_row(collector, header, header_top, &column_widths, spacing);
        }

        let row_top = collector.reserve(prepared.height, space_before);
        emit_prepared_row(collector, &prepared, row_top, &column_widths, spacing);
        if spacing > 0.0 {
            collector.cursor += spacing;
        }
        if is_header && header_row.is_none() {
            header_row = Some(prepared);
        }
    }
}

fn layout_float<'a>(
    element: &ElementRef<'a>,
    style: &Style,
    rules: &[Rule],
    collector: &mut Collector,
) {
    layout_overlay(element, style, rules, collector, Some(style.float));
}

fn layout_overlay<'a>(
    element: &ElementRef<'a>,
    style: &Style,
    rules: &[Rule],
    collector: &mut Collector,
    float: Option<Float>,
) {
    let saved_max = collector.max_width;
    let saved_page_height = collector.page_height;
    let saved_cursor = collector.cursor;
    let saved_align = collector.align;
    let saved_pending = collector.pending_before;
    let saved_block = collector.block.clone();
    let saved_stack_len = collector.style_stack.len();
    let saved_pages = collector.pages.len();
    let saved_anchors = collector.anchors.len();
    let saved_bookmarks = collector.bookmarks.len();

    let content_width = (collector.max_width * 0.6).max(1.0);
    collector.max_width = content_width;
    collector.page_height = f32::MAX;
    collector.cursor = 0.0;
    collector.new_page();
    collector.align = style.text_align;
    collector.pending_before = 0.0;
    collector.block = BlockStyle {
        background: style.background,
        background_alpha: style.background_alpha,
        border: style.border,
        opacity: style.opacity,
        border_radius: style.border_radius,
        ..BlockStyle::default()
    };

    for child in element.children() {
        if let Some(child_element) = ElementRef::wrap(child) {
            walk(child_element, style, rules, collector);
        } else if let Some(text) = child.value().as_text() {
            collector.push_text(text, style);
        }
    }
    collector.flush_line();

    let mut lines = collector.pages.pop().expect("overlay page").lines;
    for line in &mut lines {
        line.opacity *= style.opacity;
    }
    collector.pages.truncate(saved_pages);
    collector.anchors.truncate(saved_anchors);
    collector.bookmarks.truncate(saved_bookmarks);
    collector.style_stack.truncate(saved_stack_len);
    collector.max_width = saved_max;
    collector.page_height = saved_page_height;
    collector.cursor = saved_cursor;
    collector.align = saved_align;
    collector.pending_before = saved_pending;
    collector.block = saved_block;

    let padding_left = style.padding_left_pt();
    let padding_right = style.padding_right_pt();
    let padding_top = style.padding_top_pt();
    let padding_bottom = style.padding_bottom_pt();
    let text_width = lines.iter().map(|line| line.width).fold(0.0f32, f32::max);
    let content_height = lines
        .iter()
        .map(|line| line.space_before + line.height + line.space_after)
        .sum::<f32>();
    let box_width = (text_width + padding_left + padding_right).max(54.0);
    let box_height = (content_height + padding_top + padding_bottom).max(18.0);

    let resolve = |value: Option<Length>| {
        value
            .map(|length| length.resolve_with(style.font_size, style.px_scale))
            .unwrap_or(0.0)
    };
    let (x, y) = if let Some(side) = float {
        (
            if side == Float::Left {
                0.0
            } else {
                saved_max - box_width
            },
            saved_cursor,
        )
    } else {
        let x = if let Some(left) = style.offset_left {
            resolve(Some(left))
        } else if style.offset_right.is_some() {
            saved_max - box_width - resolve(style.offset_right)
        } else {
            0.0
        };
        let y = if let Some(top) = style.offset_top {
            resolve(Some(top))
        } else if style.offset_bottom.is_some() {
            saved_page_height - box_height - resolve(style.offset_bottom)
        } else {
            0.0
        };
        (x, y)
    };

    let background_image = style
        .background_image
        .as_deref()
        .and_then(|url| collector.load_image_index(url));

    let page = collector.pages.last_mut().expect("page exists");
    page.overlays.push(Overlay {
        z: style.z_index,
        line: Line {
            items: Vec::new(),
            width: box_width,
            height: box_height,
            space_before: 0.0,
            space_after: 0.0,
            top: y,
            left: x,
            align: TextAlign::Left,
            background: style.background,
            background_alpha: style.background_alpha,
            background_image,
            background_repeat: style.background_repeat,
            background_size: style.background_size,
            background_position_x: style.background_position_x,
            background_position_y: style.background_position_y,
            background_width: Some(box_width),
            border: style.border,
            opacity: style.opacity,
            border_radius: style.border_radius,
            links: Vec::new(),
        },
    });

    let overlays = &mut collector.pages.last_mut().expect("page exists").overlays;
    for mut line in lines {
        line.top += y + padding_top;
        line.left += x + padding_left;
        overlays.push(Overlay {
            z: style.z_index,
            line,
        });
    }

    if let Some(side) = float {
        let bottom = y + box_height;
        match side {
            Float::Left => collector.float_left_bottom = collector.float_left_bottom.max(bottom),
            Float::Right => collector.float_right_bottom = collector.float_right_bottom.max(bottom),
            Float::None => {}
        }
    }
}

fn parse_positive_attr(value: Option<&str>) -> Option<usize> {
    value?.parse::<usize>().ok().filter(|value| *value > 0)
}

fn parse_dimension(value: &str, px_scale: f32) -> Option<f32> {
    let text = value.trim().to_ascii_lowercase();
    let (number, scale) = if let Some(number) = text.strip_suffix("px") {
        (number, px_scale)
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
