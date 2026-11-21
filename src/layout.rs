use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use scraper::{ElementRef, Html, Selector};
use ttf_parser::Face;

use crate::css::{
    BackgroundRepeat, BackgroundSize, BorderCollapse, BorderSpec, Clear, CounterStyle,
    CounterSystem, Display, Float, FontFace, Length, ListStyle, MediaContext, PageRules, Position,
    PseudoElement, Rule, Style, TextAlign, VerticalAlign, WhiteSpace, parse_color,
    parse_counter_styles, parse_font_faces, parse_length, parse_page_rules, parse_styles,
    pseudo_style_for, px_scale, style_for,
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
pub struct FormItem {
    pub control: FormControl,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub enum FormControl {
    Text {
        name: String,
        value: String,
        password: bool,
        multiline: bool,
        max_len: Option<i32>,
        required: bool,
        read_only: bool,
        js: Option<String>,
    },
    CheckBox {
        name: String,
        checked: bool,
        required: bool,
        js: Option<String>,
    },
    Radio {
        name: String,
        value: String,
        checked: bool,
        js: Option<String>,
    },
    Choice {
        name: String,
        options: Vec<String>,
        value: Option<String>,
        combo: bool,
        multiple: bool,
        js: Option<String>,
    },
    PushButton {
        name: String,
        caption: String,
        js: Option<String>,
    },
}

impl FormControl {
    pub fn field_name(&self) -> &str {
        match self {
            Self::Text { name, .. }
            | Self::CheckBox { name, .. }
            | Self::Radio { name, .. }
            | Self::Choice { name, .. }
            | Self::PushButton { name, .. } => name,
        }
    }

    pub fn display_text(&self) -> String {
        match self {
            Self::Text {
                value, password, ..
            } => {
                if *password {
                    "*".repeat(value.chars().count())
                } else {
                    value.clone()
                }
            }
            Self::CheckBox { checked, .. } => {
                if *checked {
                    "✓".to_string()
                } else {
                    String::new()
                }
            }
            Self::Radio { checked, .. } => {
                if *checked {
                    "●".to_string()
                } else {
                    String::new()
                }
            }
            Self::Choice { value, options, .. } => value
                .clone()
                .or_else(|| options.first().cloned())
                .unwrap_or_default(),
            Self::PushButton { caption, .. } => caption.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum InlineItem {
    Text(Run),
    Image(ImageItem),
    Form(FormItem),
}

#[derive(Clone, Debug)]
pub struct Script {
    pub name: String,
    pub code: String,
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
    background_position_x_is_length: bool,
    background_position_y_is_length: bool,
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
            background_position_x_is_length: false,
            background_position_y_is_length: false,
            border: None,
            opacity: 1.0,
            border_radius: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureRole {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    ListItem,
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
    pub background_position_x_is_length: bool,
    pub background_position_y_is_length: bool,
    pub background_width: Option<f32>,
    pub border: Option<BorderSpec>,
    pub opacity: f32,
    pub border_radius: [f32; 4],
    pub links: Vec<LinkRect>,
    pub struct_role: StructureRole,
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
    pub is_cff: bool,
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
    pub counter_styles: Vec<CounterStyle>,
    pub scripts: Vec<Script>,
    pub lang: Option<String>,
    pub metadata: Metadata,
    pub width: f32,
    pub height: f32,
    pub margin: f32,
    pub page_text: Option<String>,
    pub page_text_location: PageTextLocation,
    pub user_password: Option<String>,
    pub owner_password: Option<String>,
    pub pdf_a: bool,
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
        let lang = parse_lang(&parsed);
        let scripts = collect_scripts(&parsed);
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
        let counter_styles = parse_counter_styles(&parsed, &base, &media, &options.resource);
        let base_style = Style {
            font_family: options.default_font.clone(),
            px_scale: px_scale(options.dpi),
            containing_width: (options.width - 2.0 * options.margin).max(1.0),
            containing_height: (options.height - 2.0 * options.margin).max(1.0),
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
            counter_styles.clone(),
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
            counter_styles: collector.counter_styles,
            scripts,
            lang,
            metadata,
            width: options.width,
            height: options.height,
            margin: options.margin,
            page_text: options.page_text.clone(),
            page_text_location: options.page_text_location,
            user_password: options.user_password.clone(),
            owner_password: options.owner_password.clone(),
            pdf_a: options.pdf_a,
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

fn collect_scripts(html: &Html) -> Vec<Script> {
    let mut scripts = Vec::new();
    let mut inline = 0usize;

    for node in html.root_element().descendants() {
        let Some(element) = ElementRef::wrap(node) else {
            continue;
        };
        if element.value().name() == "script" {
            let code = element.text().collect::<String>().trim().to_string();
            if !code.is_empty() {
                scripts.push(Script {
                    name: format!("Script{}", scripts.len() + 1),
                    code,
                });
            }
        }
        for (key, value) in element.value().attrs() {
            if key.starts_with("on") && !value.trim().is_empty() {
                inline += 1;
                scripts.push(Script {
                    name: format!("Inline{inline}"),
                    code: value.to_string(),
                });
            }
        }
    }

    scripts
}

fn inline_script(element: &ElementRef) -> Option<String> {
    let handlers = element
        .value()
        .attrs()
        .filter(|(key, value)| key.starts_with("on") && !value.trim().is_empty())
        .map(|(_, value)| value.to_string())
        .collect::<Vec<_>>();
    (!handlers.is_empty()).then(|| handlers.join(";"))
}

fn element_text(element: &ElementRef) -> String {
    let mut text = String::new();
    collect_element_text(*element, &mut text);
    normalize_title(&text)
}

fn form_control(
    element: &ElementRef,
    style: &Style,
    collector: &Collector,
    width: &mut f32,
    height: &mut f32,
) -> FormControl {
    let tag = element.value().name();
    let input_type = element.attr("type").unwrap_or("text").to_ascii_lowercase();
    let js = inline_script(element);
    let name = element
        .attr("name")
        .or_else(|| element.attr("id"))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("field{}", collector.form_count + 1));
    let required = element.attr("required").is_some();
    let read_only = element.attr("readonly").is_some();
    let max_len = element
        .attr("maxlength")
        .and_then(|value| value.parse().ok());
    let mut value = element.attr("value").unwrap_or_default().to_string();
    if value.is_empty() && matches!(tag, "textarea" | "button") {
        value = element_text(element);
    }

    match tag {
        "textarea" => {
            *width = element
                .attr("cols")
                .and_then(|value| value.parse::<f32>().ok())
                .map(|cols| cols * style.font_size * 0.6)
                .unwrap_or(160.0);
            *height = element
                .attr("rows")
                .and_then(|value| value.parse::<f32>().ok())
                .map(|rows| rows * style.font_size * 1.4)
                .unwrap_or(48.0);
            FormControl::Text {
                name,
                value,
                password: false,
                multiline: true,
                max_len,
                required,
                read_only,
                js,
            }
        }
        "select" => {
            let mut options = Vec::new();
            let mut selected = None;
            for node in element.descendants() {
                let Some(option) = ElementRef::wrap(node) else {
                    continue;
                };
                if option.value().name() != "option" {
                    continue;
                }
                let label = option.text().collect::<String>().trim().to_string();
                if label.is_empty() {
                    continue;
                }
                if option.attr("selected").is_some() && selected.is_none() {
                    selected = Some(label.clone());
                }
                options.push(label);
            }
            *width = options
                .iter()
                .map(|label| label.chars().count())
                .max()
                .map(|len| (len.max(8) as f32) * style.font_size * 0.6 + 18.0)
                .unwrap_or(120.0)
                .min(240.0);
            *height = 20.0;
            FormControl::Choice {
                name,
                options,
                value: selected.or_else(|| (!value.is_empty()).then_some(value)),
                combo: element.attr("multiple").is_none(),
                multiple: element.attr("multiple").is_some(),
                js,
            }
        }
        "button" => {
            let caption = if value.is_empty() {
                "Button".to_string()
            } else {
                value
            };
            *width = (caption.chars().count().max(6) as f32) * style.font_size * 0.6 + 16.0;
            *height = 20.0;
            FormControl::PushButton { name, caption, js }
        }
        _ => match input_type.as_str() {
            "checkbox" => {
                *width = 14.0;
                *height = 14.0;
                FormControl::CheckBox {
                    name,
                    checked: element.attr("checked").is_some(),
                    required,
                    js,
                }
            }
            "radio" => {
                *width = 14.0;
                *height = 14.0;
                FormControl::Radio {
                    name,
                    value,
                    checked: element.attr("checked").is_some(),
                    js,
                }
            }
            "submit" | "button" | "reset" => {
                let caption = if value.is_empty() {
                    match input_type.as_str() {
                        "submit" => "Submit",
                        "reset" => "Reset",
                        _ => "Button",
                    }
                    .to_string()
                } else {
                    value
                };
                *width = (caption.chars().count().max(6) as f32) * style.font_size * 0.6 + 16.0;
                *height = 20.0;
                FormControl::PushButton { name, caption, js }
            }
            _ => {
                if value.is_empty() {
                    value = element_text(element);
                }
                *width = element
                    .attr("size")
                    .and_then(|value| value.parse::<f32>().ok())
                    .map(|size| size * style.font_size * 0.6)
                    .unwrap_or(120.0);
                *height = if input_type == "hidden" { 0.0 } else { 20.0 };
                FormControl::Text {
                    name,
                    value,
                    password: input_type == "password",
                    multiline: false,
                    max_len,
                    required,
                    read_only,
                    js,
                }
            }
        },
    }
}

fn parse_lang(html: &Html) -> Option<String> {
    html.root_element()
        .attr("lang")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
struct FloatRect {
    side: Float,
    left: f32,
    width: f32,
    top: f32,
    bottom: f32,
}

#[derive(Clone, Debug)]
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
    floats: Vec<FloatRect>,
    float_indent: f32,
    line_max_width: f32,
    align: TextAlign,
    max_width: f32,
    page_height: f32,
    cursor: f32,
    base: PathBuf,
    images: Vec<ImageAsset>,
    image_cache: HashMap<String, Option<usize>>,
    fonts: Vec<EmbeddedFont>,
    font_cache: HashMap<String, Option<usize>>,
    counter_styles: Vec<CounterStyle>,
    block: BlockStyle,
    style_stack: Vec<BlockStyle>,
    link: Option<String>,
    lists: Vec<ListState>,
    struct_role: StructureRole,
    form_count: usize,
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
        counter_styles: Vec<CounterStyle>,
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
            floats: Vec::new(),
            float_indent: 0.0,
            line_max_width: content_width,
            align: TextAlign::Left,
            max_width: content_width,
            page_height: content_height,
            cursor: 0.0,
            base: base.to_path_buf(),
            images: Vec::new(),
            image_cache: HashMap::new(),
            fonts: Vec::new(),
            font_cache: HashMap::new(),
            counter_styles,
            block: BlockStyle::default(),
            style_stack: Vec::new(),
            link: None,
            lists: Vec::new(),
            struct_role: StructureRole::Paragraph,
            form_count: 0,
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
        if self.current.is_empty() {
            self.line_max_width = self.max_width;
        }
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
        self.floats.clear();
        self.line_max_width = self.max_width;
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
            line.background_position_x_is_length = self.block.background_position_x_is_length;
            line.background_position_y_is_length = self.block.background_position_y_is_length;
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
        line.left = self.pending_indent + self.offset_x + self.float_indent;
        self.pending_indent = 0.0;
        self.float_indent = 0.0;
        self.line_max_width = self.max_width;
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
            background_position_x_is_length: false,
            background_position_y_is_length: false,
            background_width: None,
            border: None,
            opacity: 1.0,
            border_radius: [0.0; 4],
            links: Vec::new(),
            struct_role: self.struct_role,
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
        if let Some((name, value)) = &style.counter_set {
            let values = self.counters.entry(name.clone()).or_default();
            if let Some(last) = values.last_mut() {
                *last = *value;
            } else {
                values.push(*value);
            }
            if !reset_names.contains(name) {
                reset_names.push(name.clone());
            }
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
        self.block.background_position_x_is_length = style.background_position_x_is_length;
        self.block.background_position_y_is_length = style.background_position_y_is_length;
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

    fn prepare_line(&mut self, line_height: f32) {
        if !self.current.is_empty() {
            return;
        }
        let top = self.cursor + self.offset_y + self.pending_before;
        let base_left = self.pending_indent + self.offset_x;
        let mut left = 0.0f32;
        let mut right = self.max_width;
        for float in &self.floats {
            if top < float.bottom && top + line_height > float.top {
                match float.side {
                    Float::Left => {
                        left = left.max(float.left + float.width - base_left);
                    }
                    Float::Right => {
                        right = right.min(float.left - base_left);
                    }
                    Float::None => {}
                }
            }
        }
        self.float_indent = left.max(0.0);
        self.line_max_width = (right - left).max(self.max_width * 0.2).max(1.0);
    }

    fn push_text(&mut self, text: &str, style: &Style) {
        if style.white_space == WhiteSpace::Pre {
            self.push_text_pre(text, style);
            return;
        }

        let space_width = self.text_width(" ", style);
        let wrap = style.white_space != WhiteSpace::Nowrap;

        let line_height = style.font_size * style.line_height;
        for word in text.split_whitespace() {
            self.prepare_line(line_height);
            let word_width = self.text_width(word, style);
            let extra = if self.current.is_empty() {
                0.0
            } else {
                space_width
            };

            if wrap
                && !self.current.is_empty()
                && self.width + extra + word_width > self.line_max_width
            {
                self.flush_line();
                self.prepare_line(line_height);
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

        let line_height = height.max(style.font_size * style.line_height);
        self.prepare_line(line_height);
        if !self.current.is_empty() && self.width + width > self.line_max_width {
            self.flush_line();
            self.prepare_line(line_height);
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

    fn push_form_item(&mut self, control: FormControl, width: f32, height: f32, style: &Style) {
        let line_height = height.max(style.font_size * style.line_height);
        self.prepare_line(line_height);
        if !self.current.is_empty() && self.width + width > self.line_max_width {
            self.flush_line();
            self.prepare_line(line_height);
        }
        self.current.push(InlineItem::Form(FormItem {
            control,
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

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn decode_font_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    match bytes.get(0..4)? {
        b"wOFF" => decode_woff1(bytes),
        b"wOF2" => decode_woff2(bytes),
        _ => Some(bytes.to_vec()),
    }
}

fn decode_woff1(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 44 {
        return None;
    }
    let flavor = read_u32(bytes, 4)?;
    let num_tables = read_u16(bytes, 12)? as usize;
    let total_sfnt_size = read_u32(bytes, 16)? as usize;
    let mut records = Vec::with_capacity(num_tables);

    for index in 0..num_tables {
        let base = 44 + index * 20;
        let tag = read_u32(bytes, base)?;
        let offset = read_u32(bytes, base + 4)? as usize;
        let compressed = read_u32(bytes, base + 8)? as usize;
        let original = read_u32(bytes, base + 12)? as usize;
        let checksum = read_u32(bytes, base + 16)?;
        let source = bytes.get(offset..offset.checked_add(compressed)?)?;
        let data = if compressed < original {
            let mut decoder = flate2::read::ZlibDecoder::new(source);
            let mut data = Vec::with_capacity(original);
            decoder.read_to_end(&mut data).ok()?;
            data
        } else {
            source.to_vec()
        };
        if data.len() != original {
            return None;
        }
        records.push((tag, checksum, data));
    }
    records.sort_by_key(|record| record.0);

    let mut search_range = 16usize;
    let mut entry_selector = 0u16;
    while search_range * 2 <= num_tables * 16 {
        search_range *= 2;
        entry_selector += 1;
    }
    let range_shift = (num_tables * 16).saturating_sub(search_range);
    let mut output = Vec::with_capacity(total_sfnt_size.max(12 + num_tables * 16));
    push_u32(&mut output, flavor);
    push_u16(&mut output, num_tables as u16);
    push_u16(&mut output, search_range as u16);
    push_u16(&mut output, entry_selector);
    push_u16(&mut output, range_shift as u16);

    let mut offset = 12 + num_tables * 16;
    for (tag, checksum, data) in &records {
        push_u32(&mut output, *tag);
        push_u32(&mut output, *checksum);
        push_u32(&mut output, offset as u32);
        push_u32(&mut output, data.len() as u32);
        offset += data.len() + (4 - data.len() % 4) % 4;
    }
    for (_, _, data) in &records {
        output.extend_from_slice(data);
        let padding = (4 - data.len() % 4) % 4;
        output.resize(output.len() + padding, 0);
    }

    Some(output)
}

struct Woff2Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Woff2Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn rest(&self) -> &'a [u8] {
        self.data.get(self.pos..).unwrap_or_default()
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.pos = self.pos.checked_add(count)?;
        if self.pos <= self.data.len() {
            Some(())
        } else {
            None
        }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let slice = self.data.get(self.pos..self.pos.checked_add(count)?)?;
        self.pos += count;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        let value = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(value)
    }

    fn u16(&mut self) -> Option<u16> {
        let value = read_u16(self.data, self.pos)?;
        self.pos += 2;
        Some(value)
    }

    fn i16(&mut self) -> Option<i16> {
        Some(self.u16()? as i16)
    }

    fn u32(&mut self) -> Option<u32> {
        let value = read_u32(self.data, self.pos)?;
        self.pos += 4;
        Some(value)
    }

    fn u255(&mut self) -> Option<u16> {
        match self.u8()? {
            253 => Some(((self.u8()? as u16) << 8) | self.u8()? as u16),
            254 => self.u8().map(|value| value as u16 + 506),
            255 => self.u8().map(|value| value as u16 + 253),
            value => Some(value as u16),
        }
    }
}

type Woff2GlyfResult = (Vec<u8>, Vec<u8>, Vec<i16>, usize);

#[derive(Clone)]
struct Woff2Table {
    tag: u32,
    encoded_len: u32,
    transform: u8,
    data: Option<Vec<u8>>,
}

fn decode_woff2(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 48 {
        return None;
    }
    let flavor = read_u32(bytes, 4)?;
    let num_tables = read_u16(bytes, 12)? as usize;
    let total_compressed = read_u32(bytes, 20)? as usize;
    let mut dir_pos = 48usize;
    let mut tables = Vec::with_capacity(num_tables);

    for _ in 0..num_tables {
        let flags = *bytes.get(dir_pos)?;
        dir_pos += 1;
        let tag = if flags & 0x3f == 63 {
            let tag = read_u32(bytes, dir_pos)?;
            dir_pos += 4;
            tag
        } else {
            woff2_known_tag((flags & 0x3f) as usize)?
        };
        let transform = flags >> 6;
        let valid = match tag {
            tag if tag == tag::GLYF || tag == tag::LOCA => transform == 0 || transform == 3,
            tag if tag == tag::HMTX => transform == 0 || transform == 1,
            _ => transform == 0,
        };
        if !valid {
            return None;
        }
        let orig_len = read_woff2_base128(bytes, &mut dir_pos)?;
        let non_null = if tag == tag::GLYF || tag == tag::LOCA {
            transform != 3
        } else {
            transform != 0
        };
        let transform_len = if non_null {
            read_woff2_base128(bytes, &mut dir_pos)?
        } else {
            0
        };
        if tag == tag::LOCA && transform == 0 && transform_len != 0 {
            return None;
        }
        let encoded_len = if non_null { transform_len } else { orig_len };
        tables.push(Woff2Table {
            tag,
            encoded_len,
            transform,
            data: None,
        });
    }

    let compressed = bytes.get(dir_pos..dir_pos.checked_add(total_compressed)?)?;
    let mut decompressed = Vec::new();
    brotli::Decompressor::new(compressed, 4096)
        .read_to_end(&mut decompressed)
        .ok()?;
    let mut stream = Woff2Reader::new(&decompressed);
    let mut glyf_result: Option<Woff2GlyfResult> = None;

    for table in &mut tables {
        if table.tag == tag::LOCA && table.transform == 0 {
            continue;
        }
        let encoded = stream.take(table.encoded_len as usize)?;
        if table.tag == tag::GLYF && table.transform == 0 {
            glyf_result = Some(inverse_woff2_glyf(encoded)?);
        } else {
            table.data = Some(encoded.to_vec());
        }
    }

    if let Some((glyf, loca, xmins, num_glyphs)) = glyf_result {
        for table in &mut tables {
            if table.tag == tag::GLYF {
                table.data = Some(glyf.clone());
            } else if table.tag == tag::LOCA {
                table.data = Some(loca.clone());
            }
        }
        let maxp = tables
            .iter()
            .find(|table| table.tag == tag::MAXP)
            .and_then(|table| table.data.as_deref())
            .and_then(|data| read_u16(data, 4))
            .unwrap_or(num_glyphs as u16) as usize;
        let hhea = tables
            .iter()
            .find(|table| table.tag == tag::HHEA)
            .and_then(|table| table.data.as_deref())
            .and_then(|data| read_u16(data, 34))
            .unwrap_or(maxp as u16) as usize;
        for table in &mut tables {
            if table.tag == tag::HMTX && table.transform == 1 {
                let encoded = table.data.take()?;
                table.data = Some(inverse_woff2_hmtx(&encoded, maxp, hhea, &xmins)?);
            }
        }
    }

    let output_tables = tables
        .into_iter()
        .filter_map(|table| table.data.map(|data| (table.tag, data)))
        .collect::<Vec<_>>();
    build_woff2_sfnt(flavor, output_tables)
}

fn inverse_woff2_glyf(data: &[u8]) -> Option<Woff2GlyfResult> {
    let mut reader = Woff2Reader::new(data);
    reader.u16()?;
    let option_flags = reader.u16()?;
    let num_glyphs = reader.u16()? as usize;
    let index_format = reader.u16()?;
    let sizes = [
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
    ];
    let contours = reader.take(sizes[0])?;
    let points = reader.take(sizes[1])?;
    let mut points_reader = Woff2Reader::new(points);
    let flags = reader.take(sizes[2])?;
    let glyphs = reader.take(sizes[3])?;
    let mut glyph_reader = Woff2Reader::new(glyphs);
    let composites = reader.take(sizes[4])?;
    let mut composite_reader = Woff2Reader::new(composites);
    let bbox_data = reader.take(sizes[5])?;
    let bitmap_len = num_glyphs.div_ceil(32) * 4;
    let bbox_bitmap = bbox_data.get(..bitmap_len)?;
    let mut bbox_reader = Woff2Reader::new(bbox_data.get(bitmap_len..)?);
    let instructions = reader.take(sizes[6])?;
    let overlap = if option_flags & 1 != 0 {
        reader.take(num_glyphs.div_ceil(8))?
    } else {
        &[]
    };

    let mut glyf = Vec::new();
    let mut offsets = Vec::with_capacity(num_glyphs + 1);
    let mut xmins = vec![0i16; num_glyphs];
    let mut flags_pos = 0usize;
    let mut instruction_pos = 0usize;
    let mut contour_pos = 0usize;

    for (glyph, xmin) in xmins.iter_mut().enumerate() {
        offsets.push(glyf.len() as u32);
        let mut contour_reader = Woff2Reader::new(contours.get(contour_pos..)?);
        let n_contours = contour_reader.i16()?;
        contour_pos += 2;
        let explicit_bbox = bbox_bitmap
            .get(glyph >> 3)
            .is_some_and(|byte| byte & (0x80 >> (glyph & 7)) != 0);
        let bbox = if explicit_bbox {
            [
                bbox_reader.i16()?,
                bbox_reader.i16()?,
                bbox_reader.i16()?,
                bbox_reader.i16()?,
            ]
        } else {
            [0, 0, 0, 0]
        };

        if n_contours == 0 {
            if explicit_bbox {
                return None;
            }
        } else if n_contours > 0 {
            let n_contours = n_contours as usize;
            let mut end_points = Vec::with_capacity(n_contours);
            let mut point_count = 0usize;
            for _ in 0..n_contours {
                point_count = point_count.checked_add(points_reader.u255()? as usize)?;
                end_points.push((point_count - 1) as u16);
            }
            let point_flags = flags.get(flags_pos..flags_pos.checked_add(point_count)?)?;
            flags_pos += point_count;
            let mut x = 0i32;
            let mut y = 0i32;
            let mut positions = Vec::with_capacity(point_count);
            let mut output_flags = Vec::with_capacity(point_count);
            let mut x_deltas = Vec::with_capacity(point_count);
            let mut y_deltas = Vec::with_capacity(point_count);
            for &point_flag in point_flags {
                let (dx, dy, consumed) =
                    decode_woff2_triplet(point_flag & 0x7f, glyph_reader.rest())?;
                glyph_reader.skip(consumed)?;
                x += dx;
                y += dy;
                positions.push((x, y));
                x_deltas.push(dx as i16);
                y_deltas.push(dy as i16);
                output_flags.push(if point_flag & 0x80 == 0 { 1 } else { 0 });
            }
            let instruction_length = glyph_reader.u255()? as usize;
            let instructions = instructions
                .get(instruction_pos..instruction_pos.checked_add(instruction_length)?)?;
            instruction_pos += instruction_length;
            let bbox = if explicit_bbox {
                bbox
            } else {
                let x_min = positions.iter().map(|point| point.0).min().unwrap_or(0);
                let x_max = positions.iter().map(|point| point.0).max().unwrap_or(0);
                let y_min = positions.iter().map(|point| point.1).min().unwrap_or(0);
                let y_max = positions.iter().map(|point| point.1).max().unwrap_or(0);
                [x_min as i16, y_min as i16, x_max as i16, y_max as i16]
            };
            if overlap
                .get(glyph >> 3)
                .is_some_and(|byte| byte & (0x80 >> (glyph & 7)) != 0)
                && !output_flags.is_empty()
            {
                output_flags[0] |= 0x40;
            }
            push_u16(&mut glyf, n_contours as u16);
            for value in bbox {
                push_i16(&mut glyf, value);
            }
            for end in end_points {
                push_u16(&mut glyf, end);
            }
            push_u16(&mut glyf, instruction_length as u16);
            glyf.extend_from_slice(instructions);
            glyf.extend_from_slice(&output_flags);
            for delta in x_deltas {
                push_i16(&mut glyf, delta);
            }
            for delta in y_deltas {
                push_i16(&mut glyf, delta);
            }
            *xmin = bbox[0];
        } else if n_contours == -1 {
            if !explicit_bbox {
                return None;
            }
            let component_start = composite_reader.pos;
            let mut have_instructions = false;
            loop {
                let flags = composite_reader.u16()?;
                composite_reader.u16()?;
                let argument_bytes = if flags & 1 != 0 { 4 } else { 2 };
                composite_reader.skip(argument_bytes)?;
                if flags & 8 != 0 {
                    composite_reader.skip(2)?;
                } else if flags & 0x40 != 0 {
                    composite_reader.skip(4)?;
                } else if flags & 0x80 != 0 {
                    composite_reader.skip(8)?;
                }
                have_instructions |= flags & 0x100 != 0;
                if flags & 0x20 == 0 {
                    break;
                }
            }
            let component = composites.get(component_start..composite_reader.pos)?;
            let instruction_length = if have_instructions {
                glyph_reader.u255()? as usize
            } else {
                0
            };
            let instructions = instructions
                .get(instruction_pos..instruction_pos.checked_add(instruction_length)?)?;
            instruction_pos += instruction_length;
            push_u16(&mut glyf, 0xffff);
            for value in bbox {
                push_i16(&mut glyf, value);
            }
            glyf.extend_from_slice(component);
            if have_instructions {
                push_u16(&mut glyf, instruction_length as u16);
                glyf.extend_from_slice(instructions);
            }
            *xmin = bbox[0];
        } else {
            return None;
        }

        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
    }
    offsets.push(glyf.len() as u32);

    let loca = if index_format == 0 {
        let mut loca = Vec::with_capacity(offsets.len() * 2);
        for offset in offsets {
            push_u16(&mut loca, (offset / 2) as u16);
        }
        loca
    } else {
        let mut loca = Vec::with_capacity(offsets.len() * 4);
        for offset in offsets {
            push_u32(&mut loca, offset);
        }
        loca
    };
    Some((glyf, loca, xmins, num_glyphs))
}

fn decode_woff2_triplet(index: u8, data: &[u8]) -> Option<(i32, i32, usize)> {
    let index = index as usize;
    let (x, y, consumed) = match index {
        0..=9 => (0usize, 256 * (index / 2) + *data.first()? as usize, 1),
        10..=19 => (
            256 * ((index - 10) / 2) + *data.first()? as usize,
            0usize,
            1,
        ),
        20..=83 => {
            let byte = *data.first()?;
            let group = (index - 20) / 16;
            let sub = (index - 20) % 16;
            (
                1 + 16 * group + (byte >> 4) as usize,
                1 + 16 * (sub / 4) + (byte & 0x0f) as usize,
                1,
            )
        }
        84..=119 => {
            let group = (index - 84) / 12;
            let sub = (index - 84) % 12;
            (
                1 + 256 * group + *data.first()? as usize,
                1 + 256 * (sub / 4) + *data.get(1)? as usize,
                2,
            )
        }
        120..=123 => (
            ((*data.first()? as usize) << 4) | (*data.get(1)? as usize >> 4),
            ((*data.get(1)? as usize & 0x0f) << 8) | *data.get(2)? as usize,
            3,
        ),
        124..=127 => (
            ((*data.first()? as usize) << 8) | *data.get(1)? as usize,
            ((*data.get(2)? as usize) << 8) | *data.get(3)? as usize,
            4,
        ),
        _ => return None,
    };
    let x_sign = if index & 1 != 0 { 1 } else { -1 };
    let y_sign = if index & 2 != 0 { 1 } else { -1 };
    Some((x as i32 * x_sign, y as i32 * y_sign, consumed))
}

fn inverse_woff2_hmtx(
    data: &[u8],
    num_glyphs: usize,
    num_hmetrics: usize,
    xmins: &[i16],
) -> Option<Vec<u8>> {
    let mut reader = Woff2Reader::new(data);
    let flags = reader.u8()?;
    if flags & 0xfc != 0 || flags & 0x03 == 0 || num_hmetrics == 0 {
        return None;
    }
    let advances = (0..num_hmetrics)
        .map(|_| reader.u16())
        .collect::<Option<Vec<_>>>()?;
    let mut bearings = vec![0i16; num_glyphs];
    if flags & 1 == 0 {
        for bearing in bearings.iter_mut().take(num_hmetrics) {
            *bearing = reader.i16()?;
        }
    } else {
        for (index, bearing) in bearings.iter_mut().take(num_hmetrics).enumerate() {
            *bearing = xmins.get(index).copied().unwrap_or(0);
        }
    }
    if flags & 2 == 0 {
        for bearing in bearings.iter_mut().skip(num_hmetrics) {
            *bearing = reader.i16()?;
        }
    } else {
        for (index, bearing) in bearings.iter_mut().enumerate().skip(num_hmetrics) {
            *bearing = xmins.get(index).copied().unwrap_or(0);
        }
    }
    let mut output = Vec::with_capacity(num_glyphs * 2 + num_hmetrics * 2);
    for (index, advance) in advances.iter().enumerate() {
        push_u16(&mut output, *advance);
        push_i16(&mut output, bearings[index]);
    }
    for bearing in bearings.iter().skip(num_hmetrics) {
        push_i16(&mut output, *bearing);
    }
    Some(output)
}

fn read_woff2_base128(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        if index == 0 && byte == 0x80 {
            return None;
        }
        if value & 0xfe00_0000 != 0 {
            return None;
        }
        value = (value << 7) | (byte & 0x7f) as u32;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn woff2_known_tag(index: usize) -> Option<u32> {
    let tag = match index {
        0 => b"cmap",
        1 => b"head",
        2 => b"hhea",
        3 => b"hmtx",
        4 => b"maxp",
        5 => b"name",
        6 => b"OS/2",
        7 => b"post",
        8 => b"cvt ",
        9 => b"fpgm",
        10 => b"glyf",
        11 => b"loca",
        12 => b"prep",
        13 => b"CFF ",
        14 => b"VORG",
        15 => b"EBDT",
        16 => b"EBLC",
        17 => b"gasp",
        18 => b"hdmx",
        19 => b"kern",
        20 => b"LTSH",
        21 => b"PCLT",
        22 => b"VDMX",
        23 => b"vhea",
        24 => b"vmtx",
        25 => b"BASE",
        26 => b"GDEF",
        27 => b"GPOS",
        28 => b"GSUB",
        29 => b"EBSC",
        30 => b"JSTF",
        31 => b"MATH",
        32 => b"CBDT",
        33 => b"CBLC",
        34 => b"COLR",
        35 => b"CPAL",
        36 => b"SVG ",
        37 => b"sbix",
        38 => b"acnt",
        39 => b"avar",
        40 => b"bdat",
        41 => b"bloc",
        42 => b"bsln",
        43 => b"cvar",
        44 => b"fdsc",
        45 => b"feat",
        46 => b"fmtx",
        47 => b"fvar",
        48 => b"gvar",
        49 => b"hsty",
        50 => b"just",
        51 => b"lcar",
        52 => b"mort",
        53 => b"morx",
        54 => b"opbd",
        55 => b"prop",
        56 => b"trak",
        57 => b"Zapf",
        58 => b"Silf",
        59 => b"Glat",
        60 => b"Gloc",
        61 => b"Feat",
        62 => b"Sill",
        _ => return None,
    };
    Some(u32::from_be_bytes(*tag))
}

fn build_woff2_sfnt(flavor: u32, mut tables: Vec<(u32, Vec<u8>)>) -> Option<Vec<u8>> {
    tables.sort_by_key(|table| table.0);
    tables.dedup_by_key(|table| table.0);
    let num_tables = tables.len();
    let mut search_range = 16usize;
    let mut entry_selector = 0u16;
    while search_range * 2 <= num_tables * 16 {
        search_range *= 2;
        entry_selector += 1;
    }
    let range_shift = (num_tables * 16).saturating_sub(search_range);
    let mut prepared = Vec::with_capacity(num_tables);
    for (tag, data) in tables {
        let mut data = data;
        if tag == tag::HEAD && data.len() >= 12 {
            data[8..12].copy_from_slice(&0u32.to_be_bytes());
        }
        prepared.push((tag, data));
    }
    let mut offset = 12 + num_tables * 16;
    let mut directory = Vec::with_capacity(num_tables * 16);
    for (tag, data) in &prepared {
        push_u32(&mut directory, *tag);
        push_u32(&mut directory, sfnt_checksum(data));
        push_u32(&mut directory, offset as u32);
        push_u32(&mut directory, data.len() as u32);
        offset += data.len() + (4 - data.len() % 4) % 4;
    }
    let mut output = Vec::with_capacity(offset);
    push_u32(&mut output, flavor);
    push_u16(&mut output, num_tables as u16);
    push_u16(&mut output, search_range as u16);
    push_u16(&mut output, entry_selector);
    push_u16(&mut output, range_shift as u16);
    output.extend_from_slice(&directory);
    let mut head_offset = None;
    let mut current_offset = 12 + num_tables * 16;
    for (tag, data) in &prepared {
        if *tag == tag::HEAD {
            head_offset = Some(current_offset + 8);
        }
        output.extend_from_slice(data);
        let padding = (4 - data.len() % 4) % 4;
        output.resize(output.len() + padding, 0);
        current_offset += data.len() + padding;
    }
    if let Some(head_offset) = head_offset {
        let adjustment = 0xb1b0_afbau32.wrapping_sub(sfnt_checksum(&output));
        output[head_offset..head_offset + 4].copy_from_slice(&adjustment.to_be_bytes());
    }
    Some(output)
}

fn sfnt_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.as_chunks::<4>().0 {
        sum = sum.wrapping_add(u32::from_be_bytes(*chunk));
    }
    let remainder = &data[data.len() / 4 * 4..];
    if !remainder.is_empty() {
        let mut padded = [0u8; 4];
        padded[..remainder.len()].copy_from_slice(remainder);
        sum = sum.wrapping_add(u32::from_be_bytes(padded));
    }
    sum
}

fn push_i16(output: &mut Vec<u8>, value: i16) {
    output.extend_from_slice(&value.to_be_bytes());
}

mod tag {
    pub const GLYF: u32 = u32::from_be_bytes(*b"glyf");
    pub const LOCA: u32 = u32::from_be_bytes(*b"loca");
    pub const HMTX: u32 = u32::from_be_bytes(*b"hmtx");
    pub const HHEA: u32 = u32::from_be_bytes(*b"hhea");
    pub const MAXP: u32 = u32::from_be_bytes(*b"maxp");
    pub const HEAD: u32 = u32::from_be_bytes(*b"head");
}

fn load_font(family: &str, bytes: &[u8], bold: bool, italic: bool) -> Option<EmbeddedFont> {
    let font_bytes = decode_font_bytes(bytes)?;
    let face = Face::parse(&font_bytes, 0).ok()?;
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

    let is_cff = face.tables().cff.is_some() || face.tables().cff2.is_some();
    let ascent = face.ascender() as f32 / units_per_em as f32;
    let descent = face.descender() as f32 / units_per_em as f32;
    Some(EmbeddedFont {
        family: family.to_string(),
        bold,
        italic,
        data: font_bytes,
        is_cff,
        advance,
        glyphs,
        units_per_em,
        ascent,
        descent,
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
            InlineItem::Form(form) => (form.width, None),
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

fn list_marker(style: &ListStyle, value: i64, styles: &[CounterStyle]) -> Option<String> {
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
        ListStyle::Custom(name) => format_counter_value(value as i32, name, styles),
    };

    (!marker.is_empty()).then_some(marker)
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

fn custom_counter_symbol(value: i32, style: &CounterStyle) -> String {
    let base = style.symbols.len().max(1);
    let index = |value: i32| ((value - 1).rem_euclid(base as i32)) as usize;
    match style.system {
        CounterSystem::Cyclic | CounterSystem::Symbolic => style
            .symbols
            .get(index(value.max(1)))
            .cloned()
            .unwrap_or_default(),
        CounterSystem::Fixed => {
            if value >= 1 {
                style
                    .symbols
                    .get((value - 1) as usize)
                    .cloned()
                    .unwrap_or_else(|| value.to_string())
            } else {
                value.to_string()
            }
        }
        CounterSystem::Numeric => positional_symbol(value, base, &style.symbols, false),
        CounterSystem::Alphabetic => positional_symbol(value, base, &style.symbols, true),
    }
}

fn positional_symbol(value: i32, base: usize, symbols: &[String], bijective: bool) -> String {
    if value <= 0 || base <= 1 {
        return value.to_string();
    }
    let mut digits = Vec::new();
    let mut remaining = value as usize;
    while remaining > 0 {
        if bijective {
            remaining -= 1;
            digits.push(symbols[remaining % base].clone());
            remaining /= base;
        } else {
            digits.push(symbols[remaining % base].clone());
            remaining /= base;
        }
    }
    digits.reverse();
    digits.concat()
}

fn format_counter_value(value: i32, name: &str, styles: &[CounterStyle]) -> String {
    let name = name.trim().trim_matches(['"', '\'']);
    if let Some(style) = styles.iter().find(|style| style.name == name) {
        return format!(
            "{}{}{}",
            style.prefix,
            custom_counter_symbol(value, style),
            style.suffix
        );
    }

    match name {
        "disc" => "•".to_string(),
        "circle" => "○".to_string(),
        "square" => "▪".to_string(),
        "none" => String::new(),
        "lower-alpha" | "lower-latin" => dec2base26(value as i64, true),
        "upper-alpha" | "upper-latin" => dec2base26(value as i64, false),
        "lower-roman" => dec2roman(value as i64).to_ascii_lowercase(),
        "upper-roman" => dec2roman(value as i64),
        _ => value.to_string(),
    }
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
    if let Some((name, value)) = &style.counter_set {
        let values = counters.entry(name.clone()).or_default();
        if let Some(last) = values.last_mut() {
            *last = *value;
        } else {
            values.push(*value);
        }
    }
    increment_counter(counters, style);
}

fn format_counter(
    counters: &HashMap<String, Vec<i32>>,
    name: &str,
    style_name: &str,
    styles: &[CounterStyle],
) -> String {
    let value = counters
        .get(name)
        .and_then(|values| values.last())
        .copied()
        .unwrap_or(0);
    format_counter_value(value, style_name, styles)
}

fn format_counters(
    counters: &HashMap<String, Vec<i32>>,
    name: &str,
    separator: &str,
    style_name: &str,
    styles: &[CounterStyle],
) -> String {
    counters
        .get(name)
        .map(|values| {
            values
                .iter()
                .map(|value| format_counter_value(*value, style_name, styles))
                .collect::<Vec<_>>()
                .join(separator)
        })
        .unwrap_or_else(|| format_counter_value(0, style_name, styles))
}

fn resolve_generated_content(
    input: &str,
    element: &ElementRef,
    counters: &HashMap<String, Vec<i32>>,
    styles: &[CounterStyle],
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
            let style_name = args.next().unwrap_or("decimal").trim();
            output.push_str(&format_counters(
                counters, name, separator, style_name, styles,
            ));
            rest = after[end + 1..].trim_start();
        } else if let Some(after) = rest.strip_prefix("counter(") {
            let Some(end) = after.find(')') else {
                break;
            };
            let mut args = after[..end].split(',');
            let name = args.next().unwrap_or("").trim().trim_matches(['"', '\'']);
            let style_name = args.next().unwrap_or("decimal").trim();
            output.push_str(&format_counter(counters, name, style_name, styles));
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
        collector
            .floats
            .retain(|float| float.bottom > collector.cursor);
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

    if matches!(tag, "input" | "textarea" | "select" | "button") {
        let input_type = element.attr("type").unwrap_or("text").to_ascii_lowercase();
        if tag == "input" && input_type == "hidden" {
            collector.link = previous_link;
            return;
        }
        let mut width = 120.0;
        let mut height = 20.0;
        let control = form_control(&element, &style, collector, &mut width, &mut height);
        collector.form_count += 1;
        collector.push_form_item(control, width, height, &style);
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
            style: style.list_style.clone(),
            counter: start,
            step,
        });
    }

    let is_block = style.display == Display::Block;
    if is_block {
        collector.start_block(&style);
    }
    let previous_struct_role = collector.struct_role;
    collector.struct_role = match heading_level(tag) {
        Some(1) => StructureRole::Heading1,
        Some(2) => StructureRole::Heading2,
        Some(3) => StructureRole::Heading3,
        Some(4) => StructureRole::Heading4,
        Some(5) => StructureRole::Heading5,
        Some(6) => StructureRole::Heading6,
        _ if tag == "li" => StructureRole::ListItem,
        _ => collector.struct_role,
    };

    if tag == "li" {
        let prefix = collector
            .lists
            .last_mut()
            .and_then(|state| {
                let value = state.counter;
                state.counter += state.step;
                list_marker(&state.style, value, &collector.counter_styles)
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
    let before_content = before_style.content.as_deref().and_then(|content| {
        resolve_generated_content(
            content,
            &element,
            &before_counters,
            &collector.counter_styles,
        )
    });
    let after_content = after_style.content.as_deref().and_then(|content| {
        resolve_generated_content(
            content,
            &element,
            &after_counters,
            &collector.counter_styles,
        )
    });

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
    collector.struct_role = previous_struct_role;
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
            background_position_x_is_length: cell.style.background_position_x_is_length,
            background_position_y_is_length: cell.style.background_position_y_is_length,
            background_width: Some(cell_width),
            border: cell.style.border,
            opacity: cell.style.opacity,
            border_radius: cell.style.border_radius,
            links: Vec::new(),
            struct_role: StructureRole::Paragraph,
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
    let saved_floats = collector.floats.clone();
    let saved_float_indent = collector.float_indent;
    let saved_line_max_width = collector.line_max_width;
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
    collector.floats = saved_floats;
    collector.float_indent = saved_float_indent;
    collector.line_max_width = saved_line_max_width;
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
            background_position_x_is_length: style.background_position_x_is_length,
            background_position_y_is_length: style.background_position_y_is_length,
            background_width: Some(box_width),
            border: style.border,
            opacity: style.opacity,
            border_radius: style.border_radius,
            links: Vec::new(),
            struct_role: StructureRole::Paragraph,
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
        collector.floats.push(FloatRect {
            side,
            left: x,
            width: box_width,
            top: y,
            bottom,
        });
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

#[cfg(test)]
mod woff2_tests {
    use super::*;

    #[test]
    fn decodes_woff2_triplets() {
        assert_eq!(decode_woff2_triplet(21, &[0x21]), Some((3, -2, 1)));
        assert_eq!(decode_woff2_triplet(84, &[0x05, 0x0a]), Some((-6, -11, 2)));
        assert_eq!(
            decode_woff2_triplet(125, &[0x01, 0x02, 0x03, 0x04]),
            Some((258, -772, 4))
        );
    }

    #[test]
    fn reconstructs_empty_transformed_glyf() {
        let mut data = Vec::new();
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 2);
        push_u16(&mut data, 0);
        for size in [4u32, 0, 0, 0, 0, 4, 0] {
            push_u32(&mut data, size);
        }
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        data.extend_from_slice(&[0u8; 4]);

        let (glyf, loca, xmins, count) = inverse_woff2_glyf(&data).unwrap();
        assert!(glyf.is_empty());
        assert_eq!(loca, vec![0, 0, 0, 0, 0, 0]);
        assert_eq!(xmins, vec![0, 0]);
        assert_eq!(count, 2);
    }

    #[test]
    fn reconstructs_transformed_hmtx() {
        let mut data = vec![2u8];
        push_u16(&mut data, 500);
        push_u16(&mut data, 600);
        push_i16(&mut data, -10);
        push_i16(&mut data, 20);
        let output = inverse_woff2_hmtx(&data, 3, 2, &[100, 200, 30]).unwrap();
        let mut expected = Vec::new();
        push_u16(&mut expected, 500);
        push_i16(&mut expected, -10);
        push_u16(&mut expected, 600);
        push_i16(&mut expected, 20);
        push_i16(&mut expected, 30);
        assert_eq!(output, expected);
    }
}
