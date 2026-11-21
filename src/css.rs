use std::path::{Path, PathBuf};

use scraper::{ElementRef, Selector};

use crate::{ResourcePolicy, read_text, resource_base};

pub const DEFAULT_DPI: f32 = 96.0;

#[derive(Clone, Debug, PartialEq)]
pub struct MediaContext {
    pub media_type: String,
    pub width: f32,
    pub height: f32,
}

impl Default for MediaContext {
    fn default() -> Self {
        Self {
            media_type: "print".to_string(),
            width: 595.28 * DEFAULT_DPI / 72.0,
            height: 841.89 * DEFAULT_DPI / 72.0,
        }
    }
}

pub fn px_scale(dpi: f32) -> f32 {
    72.0 / dpi.max(1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ListStyle {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    None,
    Custom(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CounterSystem {
    #[default]
    Symbolic,
    Cyclic,
    Numeric,
    Alphabetic,
    Fixed,
}

#[derive(Clone, Debug)]
pub struct CounterStyle {
    pub name: String,
    pub system: CounterSystem,
    pub symbols: Vec<String>,
    pub prefix: String,
    pub suffix: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Pre,
    Nowrap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSpec {
    pub width: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl Default for BorderSpec {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: [0.0, 0.0, 0.0],
            alpha: 1.0,
            top: true,
            right: true,
            bottom: true,
            left: true,
        }
    }
}

impl BorderSpec {
    pub fn side(&self, index: usize) -> bool {
        match index {
            0 => self.top,
            1 => self.right,
            2 => self.bottom,
            3 => self.left,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FontFace {
    pub family: String,
    pub src: String,
    pub base: PathBuf,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PageRule {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub margin_left: Option<f32>,
}

impl PageRule {
    fn overlay(&mut self, other: &PageRule) {
        if other.width.is_some() {
            self.width = other.width;
        }
        if other.height.is_some() {
            self.height = other.height;
        }
        if other.margin.is_some() {
            self.margin = other.margin;
        }
        if other.margin_top.is_some() {
            self.margin_top = other.margin_top;
        }
        if other.margin_right.is_some() {
            self.margin_right = other.margin_right;
        }
        if other.margin_bottom.is_some() {
            self.margin_bottom = other.margin_bottom;
        }
        if other.margin_left.is_some() {
            self.margin_left = other.margin_left;
        }
    }

    pub fn margin_points(&self, fallback: f32) -> f32 {
        let sides = [
            self.margin_top.or(self.margin),
            self.margin_right.or(self.margin),
            self.margin_bottom.or(self.margin),
            self.margin_left.or(self.margin),
        ];
        let specified = sides.into_iter().flatten().collect::<Vec<_>>();
        if specified.is_empty() {
            fallback
        } else {
            specified.iter().sum::<f32>() / specified.len() as f32
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PageRules {
    pub default: PageRule,
    pub first: PageRule,
    pub left: PageRule,
    pub right: PageRule,
    pub named: std::collections::HashMap<String, PageRule>,
}

impl PageRules {
    pub fn for_index(&self, index: usize, name: Option<&str>) -> PageRule {
        let mut rule = self.default.clone();
        let pseudo = if index == 0 {
            None
        } else if index % 2 == 1 {
            Some(&self.right)
        } else {
            Some(&self.left)
        };
        if let Some(pseudo) = pseudo {
            rule.overlay(pseudo);
        }
        if index == 0 {
            rule.overlay(&self.first);
        }
        if let Some(name) = name
            && let Some(named) = self.named.get(name)
        {
            rule.overlay(named);
        }
        rule
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Length {
    pub value: f32,
    pub unit: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Pt,
    Px,
    Em,
    In,
    Cm,
    Mm,
    Percent,
}

impl Default for Length {
    fn default() -> Self {
        Self {
            value: 0.0,
            unit: Unit::Pt,
        }
    }
}

impl Length {
    pub fn resolve(self, font_size: f32) -> f32 {
        self.resolve_with(font_size, px_scale(DEFAULT_DPI))
    }

    pub fn resolve_with(self, font_size: f32, scale: f32) -> f32 {
        self.resolve_with_size(font_size, font_size, scale)
    }
}

#[derive(Clone, Debug)]
pub struct Style {
    pub color: [f32; 3],
    pub color_alpha: f32,
    pub background_alpha: f32,
    pub font_family: String,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    pub line_height: f32,
    pub margin_top: Length,
    pub margin_right: Length,
    pub margin_bottom: Length,
    pub margin_left: Length,
    pub padding_top: Length,
    pub padding_right: Length,
    pub padding_bottom: Length,
    pub padding_left: Length,
    pub background: Option<[f32; 3]>,
    pub background_image: Option<String>,
    pub background_repeat: BackgroundRepeat,
    pub background_size: BackgroundSize,
    pub background_position_x: f32,
    pub background_position_y: f32,
    pub background_position_x_is_length: bool,
    pub background_position_y_is_length: bool,
    pub border: Option<BorderSpec>,
    pub opacity: f32,
    pub border_radius: [f32; 4],
    pub border_collapse: BorderCollapse,
    pub border_spacing: f32,
    pub vertical_align: VerticalAlign,
    pub text_align: TextAlign,
    pub display: Display,
    pub px_scale: f32,
    pub containing_width: f32,
    pub containing_height: f32,
    pub list_style: ListStyle,
    pub text_indent: Length,
    pub letter_spacing: Length,
    pub white_space: WhiteSpace,
    pub page_name: Option<String>,
    pub content: Option<String>,
    pub counter_reset: Vec<(String, i32)>,
    pub counter_set: Vec<(String, i32)>,
    pub counter_increment: Vec<(String, i32)>,
    pub position: Position,
    pub float: Float,
    pub clear: Clear,
    pub z_index: i32,
    pub offset_left: Option<Length>,
    pub offset_right: Option<Length>,
    pub offset_top: Option<Length>,
    pub offset_bottom: Option<Length>,
    pub variables: std::collections::HashMap<String, String>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0],
            color_alpha: 1.0,
            background_alpha: 1.0,
            font_family: "Helvetica".to_string(),
            font_size: 12.0,
            bold: false,
            italic: false,
            underline: false,
            overline: false,
            line_through: false,
            line_height: 1.2,
            margin_top: Length::default(),
            margin_right: Length::default(),
            margin_bottom: Length::default(),
            margin_left: Length::default(),
            padding_top: Length::default(),
            padding_right: Length::default(),
            padding_bottom: Length::default(),
            padding_left: Length::default(),
            background: None,
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
            border_collapse: BorderCollapse::Separate,
            border_spacing: 0.0,
            vertical_align: VerticalAlign::Baseline,
            text_align: TextAlign::Left,
            display: Display::Inline,
            px_scale: px_scale(DEFAULT_DPI),
            containing_width: 0.0,
            containing_height: 0.0,
            list_style: ListStyle::Disc,
            text_indent: Length::default(),
            letter_spacing: Length::default(),
            white_space: WhiteSpace::Normal,
            page_name: None,
            content: None,
            counter_reset: Vec::new(),
            counter_set: Vec::new(),
            counter_increment: Vec::new(),
            position: Position::Static,
            float: Float::None,
            clear: Clear::None,
            z_index: 0,
            offset_left: None,
            offset_right: None,
            offset_top: None,
            offset_bottom: None,
            variables: std::collections::HashMap::new(),
        }
    }
}

impl Style {
    pub fn inherited(parent: &Style) -> Self {
        Self {
            color: parent.color,
            color_alpha: parent.color_alpha,
            font_family: parent.font_family.clone(),
            font_size: parent.font_size,
            bold: parent.bold,
            italic: parent.italic,
            underline: parent.underline,
            overline: parent.overline,
            line_through: parent.line_through,
            line_height: parent.line_height,
            px_scale: parent.px_scale,
            containing_width: parent.containing_width,
            containing_height: parent.containing_height,
            list_style: parent.list_style.clone(),
            text_indent: parent.text_indent,
            letter_spacing: parent.letter_spacing,
            white_space: parent.white_space,
            page_name: parent.page_name.clone(),
            variables: parent.variables.clone(),
            ..Self::default()
        }
    }

    pub fn margin_top_pt(&self) -> f32 {
        self.margin_top
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn margin_right_pt(&self) -> f32 {
        self.margin_right
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn margin_bottom_pt(&self) -> f32 {
        self.margin_bottom
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn margin_left_pt(&self) -> f32 {
        self.margin_left
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn padding_top_pt(&self) -> f32 {
        self.padding_top
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn padding_right_pt(&self) -> f32 {
        self.padding_right
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn padding_bottom_pt(&self) -> f32 {
        self.padding_bottom
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn padding_left_pt(&self) -> f32 {
        self.padding_left
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn text_indent_pt(&self) -> f32 {
        self.text_indent
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }

    pub fn letter_spacing_pt(&self) -> f32 {
        self.letter_spacing
            .resolve_with_size(self.font_size, self.containing_width, self.px_scale)
    }
}

#[derive(Clone, Debug)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundRepeat {
    Repeat,
    NoRepeat,
    RepeatX,
    RepeatY,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackgroundSize {
    Auto,
    Cover,
    Contain,
    Dimensions(Option<f32>, Option<f32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderCollapse {
    Separate,
    Collapse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlign {
    Baseline,
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PseudoElement {
    None,
    Before,
    After,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    pub specificity: u32,
    pub order: usize,
    pub origin: u32,
    pub pseudo: PseudoElement,
}

const UA_CSS: &str = include_str!("../../lib/res/html.css");

pub fn parse_styles(
    html: &scraper::Html,
    base: &Path,
    media: &MediaContext,
    policy: &ResourcePolicy,
) -> Vec<Rule> {
    let sheets = stylesheets(html, base, media, policy);
    let mut rules = Vec::new();
    let mut order = 0usize;

    rules.extend(parse_css(&ua_stylesheet(), &mut order, media, 0));
    for (text, _) in &sheets {
        rules.extend(parse_css(text, &mut order, media, 1));
    }

    rules
}

fn ua_stylesheet() -> String {
    let clean = strip_comments(UA_CSS);
    let mut output = String::new();
    let mut rest = clean.as_str();

    while rest.contains('{') {
        let Some((prelude, _, end)) = find_block(rest, 0) else {
            break;
        };
        if !prelude.starts_with("@page") {
            output.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    output.push_str(rest);
    output
}

pub fn parse_font_faces(
    html: &scraper::Html,
    base: &Path,
    media: &MediaContext,
    policy: &ResourcePolicy,
) -> Vec<FontFace> {
    let sheets = stylesheets(html, base, media, policy);
    let mut faces = Vec::new();

    for (text, sheet_base) in &sheets {
        faces.extend(parse_font_faces_css(text, sheet_base));
    }

    faces
}

pub fn parse_counter_styles(
    html: &scraper::Html,
    base: &Path,
    media: &MediaContext,
    policy: &ResourcePolicy,
) -> Vec<CounterStyle> {
    let sheets = stylesheets(html, base, media, policy);
    let mut styles = Vec::new();

    for (text, _) in &sheets {
        styles.extend(parse_counter_styles_css(text));
    }

    styles
}

pub fn parse_page_rules(
    html: &scraper::Html,
    base: &Path,
    dpi: f32,
    media: &MediaContext,
    policy: &ResourcePolicy,
) -> PageRules {
    let sheets = stylesheets(html, base, media, policy);
    let scale = px_scale(dpi);
    let mut rules = PageRules::default();

    for (text, _) in &sheets {
        collect_page_rules(text, media, scale, &mut rules);
    }

    rules
}

enum PageTarget {
    Default,
    First,
    Left,
    Right,
    Named(String),
}

fn collect_page_rules(input: &str, media: &MediaContext, scale: f32, rules: &mut PageRules) {
    let clean = strip_comments(input);
    let mut rest = clean.as_str();

    while rest.contains('{') {
        let Some((prelude, body, end)) = find_block(rest, 0) else {
            break;
        };
        rest = &rest[end..];

        if let Some(query) = prelude.strip_prefix("@media") {
            if media_matches(query.trim(), media) {
                collect_page_rules(&body, media, scale, rules);
            }
            continue;
        }

        let Some(selector) = prelude.strip_prefix("@page") else {
            continue;
        };

        for raw in selector.split(',') {
            for target in parse_page_selector(raw.trim()) {
                let page = match target {
                    PageTarget::Default => &mut rules.default,
                    PageTarget::First => &mut rules.first,
                    PageTarget::Left => &mut rules.left,
                    PageTarget::Right => &mut rules.right,
                    PageTarget::Named(name) => rules.named.entry(name).or_default(),
                };
                apply_page_declarations(&body, scale, page);
            }
        }
    }
}

fn parse_page_selector(selector: &str) -> Vec<PageTarget> {
    if selector.is_empty() || selector.eq_ignore_ascii_case("auto") {
        return vec![PageTarget::Default];
    }

    let mut name = None;
    let mut pseudo = None;
    for part in selector.split(':') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "first" => pseudo = Some(PageTarget::First),
            "left" => pseudo = Some(PageTarget::Left),
            "right" => pseudo = Some(PageTarget::Right),
            _ => name = Some(part.to_string()),
        }
    }

    let mut targets = Vec::new();
    if let Some(name) = name {
        targets.push(PageTarget::Named(name));
    }
    if let Some(pseudo) = pseudo {
        targets.push(pseudo);
    } else if targets.is_empty() {
        targets.push(PageTarget::Default);
    }
    targets
}

fn apply_page_declarations(body: &str, scale: f32, page: &mut PageRule) {
    for (property, value) in parse_declarations(body) {
        match property.as_str() {
            "size" => {
                let (dimensions, landscape) = parse_page_size(&value, scale);
                if let Some((mut width, mut height)) = dimensions {
                    if landscape {
                        std::mem::swap(&mut width, &mut height);
                    }
                    page.width = Some(width);
                    page.height = Some(height);
                }
            }
            "margin" => {
                if let Some(sides) = parse_box_shorthand(&value) {
                    let points = sides.map(|length| length.resolve_with(12.0, scale));
                    page.margin_top = Some(points[0]);
                    page.margin_right = Some(points[1]);
                    page.margin_bottom = Some(points[2]);
                    page.margin_left = Some(points[3]);
                    page.margin = Some((points[0] + points[1] + points[2] + points[3]) / 4.0);
                } else if let Some(length) = parse_length(&value) {
                    let points = length.resolve_with(12.0, scale);
                    page.margin_top = Some(points);
                    page.margin_right = Some(points);
                    page.margin_bottom = Some(points);
                    page.margin_left = Some(points);
                    page.margin = Some(points);
                }
            }
            "margin-top" => {
                page.margin_top = parse_length(&value).map(|l| l.resolve_with(12.0, scale))
            }
            "margin-right" => {
                page.margin_right = parse_length(&value).map(|l| l.resolve_with(12.0, scale))
            }
            "margin-bottom" => {
                page.margin_bottom = parse_length(&value).map(|l| l.resolve_with(12.0, scale))
            }
            "margin-left" => {
                page.margin_left = parse_length(&value).map(|l| l.resolve_with(12.0, scale))
            }
            _ => {}
        }
    }
}

fn parse_page_size(value: &str, scale: f32) -> (Option<(f32, f32)>, bool) {
    let mut named = None;
    let mut width = None;
    let mut height = None;
    let mut landscape = false;

    for token in value.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "landscape" => landscape = true,
            "portrait" => {}
            _ => {
                if let Some(size) = paper_size(&lower) {
                    named = Some(size);
                } else if let Some(length) = parse_length(token) {
                    let points = length.resolve_with(12.0, scale);
                    if width.is_none() {
                        width = Some(points);
                    } else {
                        height = Some(points);
                    }
                }
            }
        }
    }

    let dimensions = match (width, height) {
        (Some(width), Some(height)) => Some((width, height)),
        (Some(width), None) => Some((width, width)),
        (None, None) => named,
        (None, Some(height)) => Some((height, height)),
    };

    (dimensions, landscape)
}

pub fn paper_size(name: &str) -> Option<(f32, f32)> {
    let size = match name.to_ascii_lowercase().replace(' ', "").as_str() {
        "4a0" => (4767.87, 6740.79),
        "2a0" => (3370.39, 4767.87),
        "a0" => (2383.94, 3370.39),
        "a1" => (1683.78, 2383.94),
        "a2" => (1190.55, 1683.78),
        "a3" => (841.89, 1190.55),
        "a4" => (595.28, 841.89),
        "a5" => (419.53, 595.28),
        "a6" => (297.64, 419.53),
        "a7" => (209.76, 297.64),
        "a8" => (147.40, 209.76),
        "a9" => (104.88, 147.40),
        "a10" => (73.70, 104.88),
        "b0" => (2834.65, 4008.19),
        "b1" => (2004.09, 2834.65),
        "b2" => (1417.32, 2004.09),
        "b3" => (1000.63, 1417.32),
        "b4" => (708.66, 1000.63),
        "b5" => (498.90, 708.66),
        "b6" => (354.33, 498.90),
        "b7" => (249.45, 354.33),
        "b8" => (175.75, 249.45),
        "b9" => (124.72, 175.75),
        "b10" => (87.87, 124.72),
        "c0" => (2599.37, 3676.54),
        "c1" => (1836.85, 2599.37),
        "c2" => (1298.27, 1836.85),
        "c3" => (918.43, 1298.27),
        "c4" => (649.13, 918.43),
        "c5" => (459.21, 649.13),
        "c6" => (323.15, 459.21),
        "c7" => (229.61, 323.15),
        "c8" => (161.57, 229.61),
        "c9" => (113.39, 161.57),
        "c10" => (79.37, 113.39),
        "ra0" => (2437.80, 3458.27),
        "ra1" => (1729.13, 2437.80),
        "ra2" => (1218.90, 1729.13),
        "ra3" => (864.57, 1218.90),
        "ra4" => (609.45, 864.57),
        "sra0" => (2551.18, 3628.35),
        "sra1" => (1814.17, 2551.18),
        "sra2" => (1275.59, 1814.17),
        "sra3" => (907.09, 1275.59),
        "sra4" => (637.80, 907.09),
        "letter" => (612.0, 792.0),
        "half-letter" => (396.0, 612.0),
        "legal" => (612.0, 1008.0),
        "ledger" => (1224.0, 792.0),
        "tabloid" => (792.0, 1224.0),
        "executive" => (521.86, 756.0),
        "folio" => (612.0, 936.0),
        "commercial#10envelope" => (684.0, 297.0),
        "catalog#101/2envelope" => (648.0, 864.0),
        "8.5x11" => (612.0, 792.0),
        "8.5x14" => (612.0, 1008.0),
        "11x17" => (792.0, 1224.0),
        _ => return None,
    };

    Some(size)
}

fn stylesheets(
    html: &scraper::Html,
    base: &Path,
    media: &MediaContext,
    policy: &ResourcePolicy,
) -> Vec<(String, PathBuf)> {
    let selector = Selector::parse("style, link").expect("valid selector");
    let mut raw = Vec::new();

    for element in html.select(&selector) {
        let value = element.value();
        match value.name() {
            "style" => raw.push((element.text().collect::<String>(), base.to_path_buf())),
            "link" => {
                let is_stylesheet = value.attr("rel").is_some_and(|rel| {
                    rel.split_whitespace()
                        .any(|item| item.eq_ignore_ascii_case("stylesheet"))
                });
                let Some(href) = value.attr("href") else {
                    continue;
                };
                if !is_stylesheet {
                    continue;
                }
                if let Some(text) = read_text(href, base, policy) {
                    raw.push((text, resource_base(href, base)));
                }
            }
            _ => {}
        }
    }

    let mut expanded = Vec::new();
    for (text, sheet_base) in raw {
        expand_css(&text, &sheet_base, media, policy, &mut expanded);
    }
    expanded
}

fn expand_css(
    input: &str,
    base: &Path,
    media: &MediaContext,
    policy: &ResourcePolicy,
    out: &mut Vec<(String, PathBuf)>,
) {
    let clean = strip_comments(input);
    let mut segments = Vec::new();
    let mut imports = Vec::new();
    let mut rest = clean.as_str();

    while let Some(position) = rest.find("@import") {
        segments.push((rest[..position].to_string(), base.to_path_buf()));
        let after = &rest[position + "@import".len()..];
        let Some(semicolon) = after.find(';') else {
            rest = &rest[position..];
            break;
        };
        let statement = &after[..semicolon];
        if let Some(url) = extract_url(statement).or_else(|| extract_quoted_value(statement))
            && media_matches(import_media(statement), media)
            && let Some(text) = read_text(&url, base, policy)
        {
            imports.push((text, resource_base(&url, base)));
        }
        rest = &after[semicolon + 1..];
    }
    segments.push((rest.to_string(), base.to_path_buf()));

    for (index, (text, segment_base)) in segments.into_iter().enumerate() {
        if !text.trim().is_empty() {
            out.push((text, segment_base));
        }
        if let Some((import, import_base)) = imports.get(index) {
            expand_css(import, import_base, media, policy, out);
        }
    }
}

fn extract_quoted_value(input: &str) -> Option<String> {
    let text = input.trim();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = text[1..].find(quote)? + 1;
    Some(text[1..end].to_string())
}

fn import_media(statement: &str) -> &str {
    if let Some(position) = statement.find(')') {
        return statement[position + 1..].trim();
    }

    let text = statement.trim();
    if let Some(quote @ ('"' | '\'')) = text.chars().next()
        && let Some(position) = text[1..].find(quote)
    {
        return text[position + 2..].trim();
    }

    ""
}

fn parse_font_faces_css(input: &str, base: &Path) -> Vec<FontFace> {
    let text = strip_comments(input);
    let mut faces = Vec::new();
    let mut rest = text.as_str();

    while let Some(position) = rest.find("@font-face") {
        let after = &rest[position + "@font-face".len()..];
        let Some(open) = after.find('{') else {
            break;
        };
        let after_open = &after[open + 1..];
        let Some(close) = after_open.find('}') else {
            break;
        };
        let declarations = parse_declarations(&after_open[..close]);
        rest = &after_open[close + 1..];

        let family = declarations
            .iter()
            .find(|(property, _)| property == "font-family")
            .and_then(|(_, value)| clean_family(value));
        let source = declarations
            .iter()
            .find(|(property, _)| property == "src")
            .and_then(|(_, value)| extract_url(value));
        let bold = declarations
            .iter()
            .find(|(property, _)| property == "font-weight")
            .is_some_and(|(_, value)| {
                let value = value.trim().to_ascii_lowercase();
                value == "bold" || value == "bolder" || value.parse::<u32>().is_ok_and(|w| w >= 600)
            });
        let italic = declarations
            .iter()
            .find(|(property, _)| property == "font-style")
            .is_some_and(|(_, value)| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "italic" | "oblique"
                )
            });

        if let (Some(family), Some(src)) = (family, source) {
            faces.push(FontFace {
                family,
                src,
                base: base.to_path_buf(),
                bold,
                italic,
            });
        }
    }

    faces
}

fn parse_counter_styles_css(input: &str) -> Vec<CounterStyle> {
    let clean = strip_comments(input);
    let mut styles = Vec::new();
    let mut rest = clean.as_str();

    while let Some((prelude, body, end)) = find_block(rest, 0) {
        rest = &rest[end..];
        let Some(name) = prelude.strip_prefix("@counter-style") else {
            continue;
        };
        let name = name.trim().trim_matches(['"', '\'']).to_string();
        if name.is_empty() {
            continue;
        }

        let mut system = CounterSystem::Symbolic;
        let mut symbols = Vec::new();
        let mut prefix = String::new();
        let mut suffix = None;
        for declaration in parse_declarations_detailed(&body) {
            match declaration.property.as_str() {
                "system" => {
                    system = match declaration.value.trim().to_ascii_lowercase().as_str() {
                        "cyclic" => CounterSystem::Cyclic,
                        "numeric" => CounterSystem::Numeric,
                        "alphabetic" => CounterSystem::Alphabetic,
                        "fixed" => CounterSystem::Fixed,
                        _ => CounterSystem::Symbolic,
                    };
                }
                "symbols" => symbols = split_css_tokens(&declaration.value),
                "prefix" => {
                    prefix = split_css_tokens(&declaration.value)
                        .first()
                        .cloned()
                        .unwrap_or_default();
                }
                "suffix" => {
                    suffix = Some(
                        split_css_tokens(&declaration.value)
                            .first()
                            .cloned()
                            .unwrap_or_default(),
                    );
                }
                _ => {}
            }
        }

        if symbols.is_empty() {
            continue;
        }
        styles.push(CounterStyle {
            name,
            system,
            symbols,
            prefix,
            suffix: suffix.unwrap_or_else(|| ". ".to_string()),
        });
    }

    styles
}

fn split_css_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in input.chars() {
        if let Some(marker) = quote {
            current.push(ch);
            if ch == marker {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                current.push(ch);
                quote = Some(ch);
            }
            ch if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(current.trim_matches(['"', '\'']).to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current.trim_matches(['"', '\'']).to_string());
    }
    tokens
}

fn clean_family(value: &str) -> Option<String> {
    let first = value.split(',').next()?.trim();
    let unquoted = first
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            first
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(first);

    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

fn parse_counters(value: &str, default: i32) -> Vec<(String, i32)> {
    if value.trim().eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let tokens: Vec<_> = value
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|token| !token.is_empty())
        .collect();
    let mut counters = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let name = tokens[index].trim_matches(['"', '\'']);
        if name.is_empty() {
            index += 1;
            continue;
        }
        let mut amount = default;
        if let Some(next) = tokens.get(index + 1)
            && let Ok(parsed) = next.parse::<i32>()
        {
            amount = parsed;
            index += 2;
        } else {
            index += 1;
        }
        counters.push((name.to_string(), amount));
    }
    counters
}

fn extract_url(value: &str) -> Option<String> {
    let start = value.find("url(")? + 4;
    let end = value[start..].find(')')? + start;
    let raw = value[start..end].trim();
    let unquoted = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(raw);

    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

pub fn parse_css(input: &str, order: &mut usize, media: &MediaContext, origin: u32) -> Vec<Rule> {
    let clean = strip_comments(input);
    let mut rules = Vec::new();
    let mut rest = clean.as_str();

    while rest.contains('{') {
        let Some((prelude, body, end)) = find_block(rest, 0) else {
            break;
        };
        rest = &rest[end..];

        if let Some(query) = prelude.strip_prefix("@media") {
            if media_matches(query.trim(), media) {
                rules.extend(parse_css(&body, order, media, origin));
            }
            continue;
        }

        if prelude.is_empty() || prelude.starts_with('@') {
            continue;
        }

        let declarations = parse_declarations_detailed(&body);
        if declarations.is_empty() {
            continue;
        }

        for raw in split_top_level(&prelude, ',') {
            let (text, pseudo) = split_pseudo_element(raw.trim());
            if let Ok(selector) = Selector::parse(text) {
                rules.push(Rule {
                    selectors: vec![selector],
                    declarations: declarations.clone(),
                    specificity: specificity(text),
                    order: *order,
                    origin,
                    pseudo,
                });
                *order += 1;
            }
        }
    }

    rules
}

fn split_pseudo_element(selector: &str) -> (&str, PseudoElement) {
    for (suffix, pseudo) in [
        ("::before", PseudoElement::Before),
        (":before", PseudoElement::Before),
        ("::after", PseudoElement::After),
        (":after", PseudoElement::After),
    ] {
        if let Some(base) = selector.strip_suffix(suffix) {
            return (base.trim(), pseudo);
        }
    }
    (selector, PseudoElement::None)
}

fn split_top_level(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for ch in input.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ch if ch == delimiter && depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn find_block(input: &str, start: usize) -> Option<(String, String, usize)> {
    let bytes = input.as_bytes();
    let mut open = start;
    while open < bytes.len() && bytes[open] != b'{' {
        open += 1;
    }
    if open == bytes.len() {
        return None;
    }

    let mut depth = 1usize;
    let mut cursor = open + 1;
    while cursor < bytes.len() && depth > 0 {
        match bytes[cursor] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    if depth != 0 {
        return None;
    }

    Some((
        input[start..open].trim().to_string(),
        input[open + 1..cursor - 1].to_string(),
        cursor,
    ))
}

fn media_matches(query: &str, context: &MediaContext) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    query
        .split(',')
        .any(|part| media_query_matches(part, context))
}

fn media_query_matches(query: &str, context: &MediaContext) -> bool {
    let tokens = tokenize_media_query(query);
    if tokens.is_empty() {
        return false;
    }

    let (negated, tokens) = match tokens.first().map(String::as_str) {
        Some("not") => (true, &tokens[1..]),
        _ => (false, tokens.as_slice()),
    };
    let tokens = tokens
        .first()
        .filter(|token| token.as_str() == "only")
        .map_or(tokens, |_| &tokens[1..]);
    if tokens.is_empty() {
        return false;
    }

    let type_matches = matches!(tokens[0].as_str(), "all") || tokens[0] == context.media_type;
    let features_match = tokens[1..]
        .iter()
        .filter(|token| token.as_str() != "and")
        .all(|token| media_feature_matches(token, context));

    if type_matches && features_match {
        !negated
    } else {
        negated
    }
}

fn tokenize_media_query(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for ch in query.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ch if ch.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn media_feature_matches(feature: &str, context: &MediaContext) -> bool {
    let inner = feature.trim().trim_start_matches('(').trim_end_matches(')');
    let Some((name, value)) = inner.split_once(':') else {
        return false;
    };
    let name = name.trim().to_ascii_lowercase();
    let value = value.trim().to_ascii_lowercase();

    if name == "orientation" {
        let landscape = context.width > context.height;
        return match value.as_str() {
            "landscape" => landscape,
            "portrait" => !landscape,
            _ => false,
        };
    }

    let Some(length) = parse_length(&value) else {
        return false;
    };
    let points = length.resolve_with(12.0, px_scale(DEFAULT_DPI));
    let pixels = points * DEFAULT_DPI / 72.0;

    match name.as_str() {
        "min-width" => context.width >= pixels,
        "max-width" => context.width <= pixels,
        "min-height" => context.height >= pixels,
        "max-height" => context.height <= pixels,
        "width" => (context.width - pixels).abs() < 0.5,
        "height" => (context.height - pixels).abs() < 0.5,
        _ => false,
    }
}

pub fn parse_declarations(input: &str) -> Vec<(String, String)> {
    parse_declarations_detailed(input)
        .into_iter()
        .map(|declaration| (declaration.property, declaration.value))
        .collect()
}

fn split_declarations(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut depth = 0i32;
    for (index, ch) in input.char_indices() {
        if let Some(marker) = quote {
            if ch == marker {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => {
                result.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    result.push(&input[start..]);
    result
}

pub fn parse_declarations_detailed(input: &str) -> Vec<Declaration> {
    split_declarations(input)
        .into_iter()
        .filter_map(|item| {
            let (property, value) = item.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim();
            let important = value.to_ascii_lowercase().ends_with("!important");
            let value = if important {
                value[..value.len() - "!important".len()].trim()
            } else {
                value
            };
            if property.is_empty() || value.is_empty() {
                None
            } else {
                Some(Declaration {
                    property,
                    value: value.to_string(),
                    important,
                })
            }
        })
        .collect()
}

type CascadedDeclaration = (bool, u32, u32, usize, Declaration);

pub fn style_for(element: &ElementRef, parent: &Style, rules: &[Rule]) -> Style {
    let tag = element.value().name();
    let mut style = style_for_tag(tag, parent);
    let mut declarations = collect_matching_declarations(element, rules, PseudoElement::None);

    if let Some(inline) = element.attr("style") {
        for declaration in parse_declarations_detailed(inline) {
            declarations.push((declaration.important, 2u32, 0, 0, declaration));
        }
    }

    for attribute in presentational_declarations(element) {
        declarations.push((false, 1u32, 0, 0, attribute));
    }

    apply_cascaded(&mut style, parent, declarations);
    style
}

pub fn pseudo_style_for(
    element: &ElementRef,
    parent: &Style,
    rules: &[Rule],
    pseudo: PseudoElement,
) -> Style {
    let mut style = Style::inherited(parent);
    let declarations = collect_matching_declarations(element, rules, pseudo);
    apply_cascaded(&mut style, parent, declarations);
    style
}

fn collect_matching_declarations(
    element: &ElementRef,
    rules: &[Rule],
    pseudo: PseudoElement,
) -> Vec<CascadedDeclaration> {
    let mut declarations = Vec::new();
    for rule in rules
        .iter()
        .filter(|rule| rule.pseudo == pseudo)
        .filter(|rule| {
            rule.selectors
                .iter()
                .any(|selector| selector.matches(element))
        })
    {
        for declaration in &rule.declarations {
            declarations.push((
                declaration.important,
                rule.origin,
                rule.specificity,
                rule.order,
                declaration.clone(),
            ));
        }
    }
    declarations
}

fn apply_cascaded(style: &mut Style, parent: &Style, mut declarations: Vec<CascadedDeclaration>) {
    declarations.sort_by_key(|(important, origin, specificity, order, _)| {
        (*important, *origin, *specificity, *order)
    });

    let values = declarations
        .into_iter()
        .map(|(_, _, _, _, declaration)| declaration)
        .collect::<Vec<_>>();

    let mut variables = parent.variables.clone();
    for declaration in &values {
        if let Some(name) = declaration.property.strip_prefix("--") {
            let name = name.trim().to_ascii_lowercase();
            let value = resolve_value(&declaration.value, &variables, 0);
            variables.insert(name, value);
        }
    }

    let resolved = values
        .into_iter()
        .map(|mut declaration| {
            declaration.value = resolve_value(&declaration.value, &variables, 0);
            declaration
        })
        .collect::<Vec<_>>();
    style.variables = variables;
    apply_style_declarations(
        style,
        &resolved,
        parent.font_size,
        parent.containing_width,
        parent.containing_height,
    );
}

fn resolve_value(
    input: &str,
    variables: &std::collections::HashMap<String, String>,
    depth: usize,
) -> String {
    if depth > 8 {
        return input.to_string();
    }

    let mut output = String::new();
    let mut rest = input;

    while let Some(position) = rest.find("var(") {
        output.push_str(&rest[..position]);
        let after = &rest[position + 4..];
        let Some(end) = find_matching_paren(after) else {
            output.push_str(&rest[position..]);
            return output;
        };
        let arguments = &after[..end];
        let (name, fallback) = arguments
            .split_once(',')
            .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
        let name = name.trim().trim_start_matches("--").to_ascii_lowercase();
        let replacement = variables
            .get(&name)
            .cloned()
            .or_else(|| fallback.map(|value| resolve_value(value, variables, depth + 1)))
            .unwrap_or_default();
        output.push_str(&replacement);
        rest = &after[end + 1..];
    }

    output.push_str(rest);
    output
}

fn find_matching_paren(input: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(index),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn presentational_declarations(element: &ElementRef) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for (attribute, property) in [("bgcolor", "background-color"), ("color", "color")] {
        if let Some(value) = element.attr(attribute) {
            declarations.push(Declaration {
                property: property.to_string(),
                value: value.to_string(),
                important: false,
            });
        }
    }
    if let Some(value) = element.attr("align")
        && let Some(align) = parse_align(value)
    {
        declarations.push(Declaration {
            property: "text-align".to_string(),
            value: align_name(align).to_string(),
            important: false,
        });
    }
    if let Some(value) = element.attr("type")
        && let Some(list) = list_style_from_html_type(value)
    {
        declarations.push(Declaration {
            property: "list-style-type".to_string(),
            value: list_style_name(list).to_string(),
            important: false,
        });
    }

    declarations
}

pub fn list_style_from_html_type(value: &str) -> Option<ListStyle> {
    let value = value.trim();
    match value {
        "a" => Some(ListStyle::LowerAlpha),
        "A" => Some(ListStyle::UpperAlpha),
        "i" => Some(ListStyle::LowerRoman),
        "I" => Some(ListStyle::UpperRoman),
        _ => match value.to_ascii_lowercase().as_str() {
            "1" | "decimal" => Some(ListStyle::Decimal),
            "disc" => Some(ListStyle::Disc),
            "circle" => Some(ListStyle::Circle),
            "square" => Some(ListStyle::Square),
            _ => None,
        },
    }
}

fn list_style_name(style: ListStyle) -> &'static str {
    match style {
        ListStyle::Disc => "disc",
        ListStyle::Circle => "circle",
        ListStyle::Square => "square",
        ListStyle::Decimal => "decimal",
        ListStyle::LowerAlpha => "lower-alpha",
        ListStyle::UpperAlpha => "upper-alpha",
        ListStyle::LowerRoman => "lower-roman",
        ListStyle::UpperRoman => "upper-roman",
        ListStyle::None => "none",
        ListStyle::Custom(_) => "disc",
    }
}

fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
        TextAlign::Justify => "justify",
    }
}

pub fn apply_declarations(style: &mut Style, declarations: &[(String, String)], parent_size: f32) {
    let detailed = declarations
        .iter()
        .map(|(property, value)| Declaration {
            property: property.clone(),
            value: value.clone(),
            important: false,
        })
        .collect::<Vec<_>>();
    apply_style_declarations(style, &detailed, parent_size, parent_size, parent_size);
}

fn apply_style_declarations(
    style: &mut Style,
    declarations: &[Declaration],
    parent_size: f32,
    containing_width: f32,
    containing_height: f32,
) {
    let mut line_height_value = None;

    for declaration in declarations {
        let (property, value) = (declaration.property.as_str(), declaration.value.as_str());
        match property {
            "color" => {
                if let Some(color) = parse_color(value) {
                    style.color = color;
                    style.color_alpha = parse_color_alpha(value).unwrap_or(1.0);
                }
            }
            "font-size" => {
                if let Some(size) = parse_font_size(value, parent_size, style.px_scale) {
                    style.font_size = size;
                }
            }
            "font-weight" => {
                style.bold = matches!(value.to_ascii_lowercase().as_str(), "bold" | "bolder")
                    || value.parse::<u32>().is_ok_and(|weight| weight >= 600);
            }
            "font-style" => {
                let lower = value.to_ascii_lowercase();
                style.italic = lower == "italic" || lower == "oblique";
            }
            "line-height" => line_height_value = Some(value.to_string()),
            "margin" => {
                if let Some([top, right, bottom, left]) = parse_box_shorthand_resolved(
                    value,
                    parent_size,
                    containing_width,
                    style.px_scale,
                ) {
                    style.margin_top = top;
                    style.margin_right = right;
                    style.margin_bottom = bottom;
                    style.margin_left = left;
                }
            }
            "margin-top" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.margin_top = length;
                }
            }
            "margin-right" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.margin_right = length;
                }
            }
            "margin-bottom" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.margin_bottom = length;
                }
            }
            "margin-left" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.margin_left = length;
                }
            }
            "page" => {
                let name = value.trim().trim_matches(['"', '\'']).to_string();
                if !name.is_empty() {
                    style.page_name = Some(name);
                }
            }
            "position" => {
                style.position = match value.trim().to_ascii_lowercase().as_str() {
                    "relative" => Position::Relative,
                    "absolute" => Position::Absolute,
                    "fixed" => Position::Fixed,
                    _ => Position::Static,
                };
            }
            "float" => {
                style.float = match value.trim().to_ascii_lowercase().as_str() {
                    "left" => Float::Left,
                    "right" => Float::Right,
                    _ => Float::None,
                };
            }
            "clear" => {
                style.clear = match value.trim().to_ascii_lowercase().as_str() {
                    "left" => Clear::Left,
                    "right" => Clear::Right,
                    "left right" | "right left" | "both" => Clear::Both,
                    _ => Clear::None,
                };
            }
            "z-index" => {
                if let Ok(value) = value.trim().parse::<i32>() {
                    style.z_index = value;
                }
            }
            "left" => {
                style.offset_left =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale);
            }
            "right" => {
                style.offset_right =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale);
            }
            "top" => {
                style.offset_top =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale);
            }
            "bottom" => {
                style.offset_bottom =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale);
            }
            "content" => {
                if value.trim().eq_ignore_ascii_case("none")
                    || value.trim().eq_ignore_ascii_case("normal")
                {
                    style.content = None;
                } else {
                    style.content = Some(value.trim().to_string());
                }
            }
            "counter-reset" => style.counter_reset = parse_counters(value, 0),
            "counter-set" => style.counter_set = parse_counters(value, 0),
            "counter-increment" => style.counter_increment = parse_counters(value, 1),
            "font-family" => {
                if let Some(family) = clean_family(value) {
                    style.font_family = family;
                }
            }
            "text-decoration" | "text-decoration-line" => {
                let decoration = value.to_ascii_lowercase();
                style.underline = decoration.contains("underline");
                style.overline = decoration.contains("overline");
                style.line_through = decoration.contains("line-through");
            }
            "background-color" => {
                style.background = parse_color(value);
                style.background_alpha = parse_color_alpha(value).unwrap_or(1.0);
            }
            "background-image" => {
                style.background_image = if value.trim().eq_ignore_ascii_case("none") {
                    None
                } else {
                    extract_url(value)
                };
            }
            "background-repeat" => {
                style.background_repeat = parse_background_repeat(value);
            }
            "background-size" => {
                if let Some(size) =
                    parse_background_size(value, style.font_size, containing_width, style.px_scale)
                {
                    style.background_size = size;
                }
            }
            "background-position" => {
                let tokens: Vec<_> = value.split_whitespace().collect();
                let resolve = |token: &str, basis: f32| match token.to_ascii_lowercase().as_str() {
                    "left" | "top" => Some((0.0, false)),
                    "center" => Some((0.5, false)),
                    "right" | "bottom" => Some((1.0, false)),
                    _ => parse_length_resolved(token, style.font_size, basis, style.px_scale).map(
                        |length| {
                            let resolved =
                                length.resolve_with_size(style.font_size, basis, style.px_scale);
                            if length.unit == Unit::Percent {
                                (resolved / basis.max(1.0), false)
                            } else {
                                (resolved, true)
                            }
                        },
                    ),
                };
                if let Some((x, is_length)) = tokens
                    .first()
                    .and_then(|token| resolve(token, containing_width))
                {
                    style.background_position_x = x;
                    style.background_position_x_is_length = is_length;
                    if tokens.len() == 1 {
                        style.background_position_y = 0.5;
                        style.background_position_y_is_length = false;
                    }
                }
                if let Some((y, is_length)) = tokens
                    .get(1)
                    .and_then(|token| resolve(token, containing_height))
                {
                    style.background_position_y = y;
                    style.background_position_y_is_length = is_length;
                }
            }
            "opacity" => {
                if let Ok(alpha) = value.trim().parse::<f32>() {
                    style.opacity = alpha.clamp(0.0, 1.0);
                }
            }
            "border-radius" => {
                if let Some(radii) =
                    parse_border_radius(value, style.font_size, containing_width, style.px_scale)
                {
                    style.border_radius = radii;
                }
            }
            "border-collapse" => {
                style.border_collapse = if value.trim().eq_ignore_ascii_case("collapse") {
                    BorderCollapse::Collapse
                } else {
                    BorderCollapse::Separate
                };
            }
            "border-spacing" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.border_spacing = length.resolve_with(style.font_size, style.px_scale);
                }
            }
            "vertical-align" => {
                style.vertical_align = match value.trim().to_ascii_lowercase().as_str() {
                    "top" => VerticalAlign::Top,
                    "middle" => VerticalAlign::Middle,
                    "bottom" => VerticalAlign::Bottom,
                    _ => VerticalAlign::Baseline,
                };
            }
            "background" => {
                if parse_color(value).is_none() {
                    for token in value.split_whitespace() {
                        if let Some(color) = parse_color(token) {
                            style.background = Some(color);
                            break;
                        }
                    }
                } else {
                    style.background = parse_color(value);
                }
                if style.background.is_some() {
                    style.background_alpha = parse_color_alpha(value).unwrap_or(1.0);
                }
                style.background_image = extract_url(value);
                for token in value.split_whitespace() {
                    match token.to_ascii_lowercase().as_str() {
                        "repeat" | "repeat-x" | "repeat-y" | "no-repeat" => {
                            style.background_repeat = parse_background_repeat(token);
                        }
                        _ => {}
                    }
                }
                if value.contains("cover") {
                    style.background_size = BackgroundSize::Cover;
                } else if value.contains("contain") {
                    style.background_size = BackgroundSize::Contain;
                }
            }
            "padding" => {
                if let Some([top, right, bottom, left]) = parse_box_shorthand_resolved(
                    value,
                    parent_size,
                    containing_width,
                    style.px_scale,
                ) {
                    style.padding_top = top;
                    style.padding_right = right;
                    style.padding_bottom = bottom;
                    style.padding_left = left;
                }
            }
            "padding-top" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.padding_top = length;
                }
            }
            "padding-right" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.padding_right = length;
                }
            }
            "padding-bottom" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.padding_bottom = length;
                }
            }
            "padding-left" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.padding_left = length;
                }
            }
            "border-color" => {
                if let Some(color) = parse_color(value) {
                    let alpha = parse_color_alpha(value).unwrap_or(1.0);
                    if let Some(border) = style.border.as_mut() {
                        border.color = color;
                        border.alpha = alpha;
                    } else {
                        style.border = Some(BorderSpec {
                            color,
                            alpha,
                            ..BorderSpec::default()
                        });
                    }
                }
            }
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                let sides = match property {
                    "border-top" => [true, false, false, false],
                    "border-right" => [false, true, false, false],
                    "border-bottom" => [false, false, true, false],
                    "border-left" => [false, false, false, true],
                    _ => [true, true, true, true],
                };

                if value.to_ascii_lowercase().contains("none") || value.trim() == "0" {
                    if let Some(existing) = style.border.as_mut() {
                        for (side, enabled) in sides.iter().enumerate() {
                            if *enabled {
                                match side {
                                    0 => existing.top = false,
                                    1 => existing.right = false,
                                    2 => existing.bottom = false,
                                    3 => existing.left = false,
                                    _ => {}
                                }
                            }
                        }
                    }
                } else if let Some(spec) = parse_border(
                    value,
                    sides,
                    style.font_size,
                    containing_width,
                    style.px_scale,
                ) {
                    style.border = Some(spec);
                }
            }
            "text-align" => {
                if let Some(align) = parse_align(value) {
                    style.text_align = align;
                }
            }
            "list-style-type" => {
                let raw = value.trim();
                style.list_style = match raw.to_ascii_lowercase().as_str() {
                    "disc" => ListStyle::Disc,
                    "circle" => ListStyle::Circle,
                    "square" => ListStyle::Square,
                    "decimal" => ListStyle::Decimal,
                    "lower-alpha" | "lower-latin" => ListStyle::LowerAlpha,
                    "upper-alpha" | "upper-latin" => ListStyle::UpperAlpha,
                    "lower-roman" => ListStyle::LowerRoman,
                    "upper-roman" => ListStyle::UpperRoman,
                    "none" => ListStyle::None,
                    _ => ListStyle::Custom(raw.trim_matches(['"', '\'']).to_string()),
                };
            }
            "text-indent" => {
                if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.text_indent = length;
                }
            }
            "letter-spacing" => {
                if matches!(value.trim().to_ascii_lowercase().as_str(), "normal") {
                    style.letter_spacing = Length::default();
                } else if let Some(length) =
                    parse_length_resolved(value, parent_size, containing_width, style.px_scale)
                {
                    style.letter_spacing = length;
                }
            }
            "white-space" => {
                style.white_space = match value.trim().to_ascii_lowercase().as_str() {
                    "pre" | "pre-line" => WhiteSpace::Pre,
                    "nowrap" => WhiteSpace::Nowrap,
                    _ => WhiteSpace::Normal,
                };
            }
            "display" => {
                style.display = match value.to_ascii_lowercase().as_str() {
                    "none" => Display::None,
                    "block" => Display::Block,
                    _ => Display::Inline,
                };
            }
            _ => {}
        }
    }

    if let Some(value) = line_height_value
        && let Some(height) = parse_line_height(&value, style.font_size, style.px_scale)
    {
        style.line_height = height;
    }
}

impl Length {
    fn resolve_font(self, parent_size: f32, scale: f32) -> f32 {
        self.resolve_with_size(parent_size, parent_size, scale)
    }

    pub fn resolve_with_size(self, font_size: f32, containing_width: f32, scale: f32) -> f32 {
        match self.unit {
            Unit::Pt => self.value,
            Unit::Px => self.value * scale,
            Unit::Em => self.value * font_size,
            Unit::In => self.value * 72.0,
            Unit::Cm => self.value * 72.0 / 2.54,
            Unit::Mm => self.value * 72.0 / 25.4,
            Unit::Percent => containing_width * self.value / 100.0,
        }
    }
}

fn parse_box_shorthand(input: &str) -> Option<[Length; 4]> {
    let values = input
        .split_whitespace()
        .map(parse_length)
        .collect::<Option<Vec<_>>>()?;

    match values.len() {
        1 => Some([values[0]; 4]),
        2 => Some([values[0], values[1], values[0], values[1]]),
        3 => Some([values[0], values[1], values[2], values[1]]),
        4 => Some([values[0], values[1], values[2], values[3]]),
        _ => None,
    }
}

fn parse_font_size(input: &str, parent_size: f32, scale: f32) -> Option<f32> {
    let text = input.trim().to_ascii_lowercase();
    if text.starts_with("calc(") && text.ends_with(')') {
        return eval_length_expression(&text[5..text.len() - 1], parent_size, parent_size, scale)
            .map(|value| value.max(0.0));
    }
    let scale = match text.as_str() {
        "xx-small" => 0.6,
        "x-small" => 0.75,
        "small" => 0.89,
        "medium" => 1.0,
        "large" => 1.2,
        "x-large" => 1.5,
        "xx-large" => 2.0,
        "xxx-large" => 3.0,
        "smaller" => 1.0 / 1.2,
        "larger" => 1.2,
        "inherit" => return None,
        _ => {
            let length = parse_length(&text)?;
            return Some(length.resolve_font(parent_size, scale).max(0.0));
        }
    };

    Some(parent_size * scale)
}

fn parse_line_height(input: &str, font_size: f32, scale: f32) -> Option<f32> {
    let text = input.trim().to_ascii_lowercase();
    if text.starts_with("calc(") && text.ends_with(')') {
        let height = eval_length_expression(&text[5..text.len() - 1], font_size, font_size, scale)?
            / font_size;
        return (height.is_finite() && height > 0.0).then_some(height);
    }
    if text == "normal" {
        return Some(1.2);
    }
    if let Ok(factor) = text.parse::<f32>() {
        return (factor > 0.0).then_some(factor);
    }

    let length = parse_length(&text)?;
    let height = length.resolve_font(font_size, scale) / font_size;
    (height.is_finite() && height > 0.0).then_some(height)
}

fn style_for_tag(tag: &str, parent: &Style) -> Style {
    let mut style = Style::inherited(parent);

    match tag {
        "html" | "body" | "div" | "section" | "article" | "header" | "footer" | "main"
        | "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" | "li" => {
            style.display = Display::Block;
        }
        "ul" => {
            style.display = Display::Block;
            style.list_style = ListStyle::Disc;
        }
        "ol" => {
            style.display = Display::Block;
            style.list_style = ListStyle::Decimal;
        }
        "p" => {
            style.display = Display::Block;
            style.margin_top = Length {
                value: 1.0,
                unit: Unit::Em,
            };
            style.margin_bottom = Length {
                value: 1.0,
                unit: Unit::Em,
            };
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = tag.as_bytes()[1] - b'0';
            let scale = match level {
                1 => 2.0,
                2 => 1.5,
                3 => 1.17,
                4 => 1.0,
                5 => 0.83,
                _ => 0.67,
            };
            style.display = Display::Block;
            style.font_size = parent.font_size * scale;
            style.bold = true;
            style.margin_top = Length {
                value: 0.67,
                unit: Unit::Em,
            };
            style.margin_bottom = Length {
                value: 0.67,
                unit: Unit::Em,
            };
        }
        "b" | "strong" => style.bold = true,
        "i" | "em" => style.italic = true,
        _ => {}
    }

    style
}

pub fn parse_length(input: &str) -> Option<Length> {
    let text = input.trim().to_ascii_lowercase();
    let (value, unit) = if let Some(value) = text.strip_suffix("px") {
        (value, Unit::Px)
    } else if let Some(value) = text.strip_suffix("pt") {
        (value, Unit::Pt)
    } else if let Some(value) = text.strip_suffix("em") {
        (value, Unit::Em)
    } else if let Some(value) = text.strip_suffix("in") {
        (value, Unit::In)
    } else if let Some(value) = text.strip_suffix("cm") {
        (value, Unit::Cm)
    } else if let Some(value) = text.strip_suffix("mm") {
        (value, Unit::Mm)
    } else if let Some(value) = text.strip_suffix('%') {
        (value, Unit::Percent)
    } else {
        (text.as_str(), Unit::Pt)
    };

    value
        .trim()
        .parse::<f32>()
        .ok()
        .map(|value| Length { value, unit })
}

fn parse_length_resolved(
    input: &str,
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> Option<Length> {
    let text = input.trim();
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("calc(") && lower.ends_with(')') {
        let expression = text.get(5..text.len() - 1)?;
        return eval_length_expression(expression, font_size, containing_width, scale).map(
            |value| Length {
                value,
                unit: Unit::Pt,
            },
        );
    }
    parse_length(text)
}

fn parse_box_shorthand_resolved(
    input: &str,
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> Option<[Length; 4]> {
    let values = tokenize_lengths(input)
        .iter()
        .map(|token| parse_length_resolved(token, font_size, containing_width, scale))
        .collect::<Option<Vec<_>>>()?;

    match values.len() {
        1 => Some([values[0]; 4]),
        2 => Some([values[0], values[1], values[0], values[1]]),
        3 => Some([values[0], values[1], values[2], values[1]]),
        4 => Some([values[0], values[1], values[2], values[3]]),
        _ => None,
    }
}

fn parse_background_repeat(input: &str) -> BackgroundRepeat {
    match input.trim().to_ascii_lowercase().replace(' ', "-").as_str() {
        "no-repeat" => BackgroundRepeat::NoRepeat,
        "repeat-x" => BackgroundRepeat::RepeatX,
        "repeat-y" => BackgroundRepeat::RepeatY,
        _ => BackgroundRepeat::Repeat,
    }
}

fn parse_background_size(
    input: &str,
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> Option<BackgroundSize> {
    let lower = input.trim().to_ascii_lowercase();
    if lower == "auto" {
        return Some(BackgroundSize::Auto);
    }
    if lower == "cover" {
        return Some(BackgroundSize::Cover);
    }
    if lower == "contain" {
        return Some(BackgroundSize::Contain);
    }

    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 2 {
        return None;
    }
    let parse_dimension = |token: &str| {
        if token.eq_ignore_ascii_case("auto") {
            return Some(None);
        }
        parse_length_resolved(token, font_size, containing_width, scale)
            .map(|length| {
                length
                    .resolve_with_size(font_size, containing_width, scale)
                    .max(0.0)
            })
            .map(Some)
    };
    let width = parse_dimension(tokens[0])?;
    let height = if let Some(token) = tokens.get(1) {
        parse_dimension(token)?
    } else {
        None
    };
    Some(BackgroundSize::Dimensions(width, height))
}

fn parse_border_radius(
    input: &str,
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> Option<[f32; 4]> {
    let values = tokenize_lengths(input)
        .iter()
        .map(|token| {
            parse_length_resolved(token, font_size, containing_width, scale).map(|length| {
                length
                    .resolve_with_size(font_size, containing_width, scale)
                    .max(0.0)
            })
        })
        .collect::<Option<Vec<_>>>()?;

    match values.len() {
        1 => Some([values[0]; 4]),
        2 => Some([values[0], values[1], values[0], values[1]]),
        3 => Some([values[0], values[1], values[2], values[1]]),
        4 => Some([values[0], values[1], values[2], values[3]]),
        _ => None,
    }
}

fn tokenize_lengths(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ch if ch.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn eval_length_expression(
    input: &str,
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> Option<f32> {
    let mut parser = CalcParser {
        input: input.chars().peekable(),
        font_size,
        containing_width,
        scale,
    };
    let value = parser.expression()?;
    parser.skip_whitespace();
    if parser.input.peek().is_some() {
        return None;
    }
    value.is_finite().then_some(value)
}

struct CalcParser<'a> {
    input: std::iter::Peekable<std::str::Chars<'a>>,
    font_size: f32,
    containing_width: f32,
    scale: f32,
}

impl CalcParser<'_> {
    fn skip_whitespace(&mut self) {
        while self.input.peek().is_some_and(|ch| ch.is_whitespace()) {
            self.input.next();
        }
    }

    fn expression(&mut self) -> Option<f32> {
        let mut value = self.term()?;
        loop {
            self.skip_whitespace();
            match self.input.peek() {
                Some('+') => {
                    self.input.next();
                    value += self.term()?;
                }
                Some('-') => {
                    self.input.next();
                    value -= self.term()?;
                }
                _ => return Some(value),
            }
        }
    }

    fn term(&mut self) -> Option<f32> {
        let mut value = self.factor()?;
        loop {
            self.skip_whitespace();
            match self.input.peek() {
                Some('*') => {
                    self.input.next();
                    value *= self.factor()?;
                }
                Some('/') => {
                    self.input.next();
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return None;
                    }
                    value /= divisor;
                }
                _ => return Some(value),
            }
        }
    }

    fn factor(&mut self) -> Option<f32> {
        self.skip_whitespace();
        match self.input.peek()? {
            '+' => {
                self.input.next();
                self.factor()
            }
            '-' => {
                self.input.next();
                Some(-self.factor()?)
            }
            '(' => {
                self.input.next();
                let value = self.expression()?;
                self.skip_whitespace();
                if self.input.next() == Some(')') {
                    Some(value)
                } else {
                    None
                }
            }
            _ => self.number_with_unit(),
        }
    }

    fn number_with_unit(&mut self) -> Option<f32> {
        let mut number = String::new();
        while self
            .input
            .peek()
            .is_some_and(|ch| ch.is_ascii_digit() || *ch == '.')
        {
            number.push(self.input.next()?);
        }
        let value = number.parse::<f32>().ok()?;

        let mut unit = String::new();
        while self
            .input
            .peek()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || *ch == '%')
        {
            unit.push(self.input.next()?);
        }

        Some(unit_to_points(
            value,
            &unit,
            self.font_size,
            self.containing_width,
            self.scale,
        ))
    }
}

fn unit_to_points(
    value: f32,
    unit: &str,
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> f32 {
    match unit.to_ascii_lowercase().as_str() {
        "px" => value * scale,
        "pt" => value,
        "in" => value * 72.0,
        "cm" => value * 72.0 / 2.54,
        "mm" => value * 72.0 / 25.4,
        "pc" => value * 12.0,
        "em" | "rem" => value * font_size,
        "%" => value * containing_width / 100.0,
        _ => value,
    }
}

fn parse_border(
    input: &str,
    sides: [bool; 4],
    font_size: f32,
    containing_width: f32,
    scale: f32,
) -> Option<BorderSpec> {
    let mut width = None;
    let mut color = parse_color(input.trim());
    let mut alpha = parse_color_alpha(input.trim()).unwrap_or(1.0);

    if color.is_none() {
        for token in input.split_whitespace() {
            if width.is_none()
                && let Some(length) = parse_length(token)
            {
                width = Some(length.resolve_with_size(font_size, containing_width, scale));
                continue;
            }
            if color.is_none()
                && let Some(parsed) = parse_color(token)
            {
                color = Some(parsed);
                alpha = parse_color_alpha(token).unwrap_or(1.0);
            }
        }
    }

    Some(BorderSpec {
        width: width.unwrap_or(1.0),
        color: color.unwrap_or([0.0, 0.0, 0.0]),
        alpha,
        top: sides[0],
        right: sides[1],
        bottom: sides[2],
        left: sides[3],
    })
}

fn parse_align(input: &str) -> Option<TextAlign> {
    match input.trim().to_ascii_lowercase().as_str() {
        "left" => Some(TextAlign::Left),
        "center" => Some(TextAlign::Center),
        "right" => Some(TextAlign::Right),
        "justify" => Some(TextAlign::Justify),
        _ => None,
    }
}

pub fn parse_color_alpha(input: &str) -> Option<f32> {
    let text = input.trim().to_ascii_lowercase();

    if let Some(hex) = text.strip_prefix('#') {
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        return match hex.len() {
            4 | 8 => u32::from_str_radix(&hex[hex.len() - 2..], 16)
                .ok()
                .map(|value| value as f32 / 255.0),
            _ => Some(1.0),
        };
    }

    if !text.contains('(') {
        return named_color(&text).map(|_| 1.0);
    }

    let (_, alpha) = split_color_channels(text.split_once('(')?.1.strip_suffix(')')?)?;
    let Some(alpha) = alpha else {
        return Some(1.0);
    };
    if let Some(percent) = alpha.trim().strip_suffix('%') {
        percent
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| (value / 100.0).clamp(0.0, 1.0))
    } else {
        alpha
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value.clamp(0.0, 1.0))
    }
}

pub fn parse_color(input: &str) -> Option<[f32; 3]> {
    let text = input.trim().to_ascii_lowercase();

    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    if !text.contains('(') {
        return named_color(&text);
    }

    let (name, arguments) = text.split_once('(')?;
    let arguments = arguments.strip_suffix(')')?;

    match name.trim() {
        "rgb" | "rgba" => parse_rgb_color(arguments),
        "hsl" | "hsla" => parse_hsl_color(arguments),
        "cmyk" => parse_cmyk_color(arguments),
        _ => None,
    }
}

fn parse_hex_color(hex: &str) -> Option<[f32; 3]> {
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let digits = match hex.len() {
        3 | 4 => hex.chars().take(3).flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => hex[..6].to_string(),
        _ => return None,
    };
    let value = u32::from_str_radix(&digits, 16).ok()?;

    Some([
        ((value >> 16) & 0xff) as f32 / 255.0,
        ((value >> 8) & 0xff) as f32 / 255.0,
        (value & 0xff) as f32 / 255.0,
    ])
}

fn split_color_channels(input: &str) -> Option<(Vec<String>, Option<String>)> {
    let input = input.trim();

    if input.contains(',') {
        let parts = input.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 3 && parts.len() != 4 {
            return None;
        }
        let channels = parts[..3].iter().map(|part| (*part).to_string()).collect();
        let alpha = parts.get(3).map(|part| (*part).to_string());
        return Some((channels, alpha));
    }

    let (values, alpha) = match input.split_once('/') {
        Some((values, alpha)) => (values, Some(alpha.trim().to_string())),
        None => (input, None),
    };
    let channels = values
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    (channels.len() == 3).then_some((channels, alpha))
}

fn parse_byte_channel(token: &str) -> Option<f32> {
    let value = if let Some(percent) = token.trim().strip_suffix('%') {
        percent.trim().parse::<f32>().ok()? * 2.55
    } else {
        token.trim().parse::<f32>().ok()?
    };

    Some(value.clamp(0.0, 255.0) / 255.0)
}

fn parse_fraction(token: &str) -> Option<f32> {
    let value = token.trim().trim_end_matches('%').parse::<f32>().ok()?;
    let fraction = if token.contains('%') || value > 1.0 {
        value / 100.0
    } else {
        value
    };

    Some(fraction.clamp(0.0, 1.0))
}

fn parse_rgb_color(input: &str) -> Option<[f32; 3]> {
    let (channels, _alpha) = split_color_channels(input)?;
    let mut rgb = [0.0; 3];
    for (slot, channel) in rgb.iter_mut().zip(channels) {
        *slot = parse_byte_channel(&channel)?;
    }

    Some(rgb)
}

fn parse_hsl_color(input: &str) -> Option<[f32; 3]> {
    let (channels, _alpha) = split_color_channels(input)?;
    let hue = channels[0]
        .trim()
        .trim_end_matches("deg")
        .parse::<f32>()
        .ok()?
        .rem_euclid(360.0);
    let saturation = parse_fraction(&channels[1])?;
    let lightness = parse_fraction(&channels[2])?;

    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let second = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let match_value = lightness - chroma / 2.0;
    let (red, green, blue) = match (hue / 60.0) as u32 % 6 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };

    Some([
        (red + match_value).clamp(0.0, 1.0),
        (green + match_value).clamp(0.0, 1.0),
        (blue + match_value).clamp(0.0, 1.0),
    ])
}

fn parse_cmyk_color(input: &str) -> Option<[f32; 3]> {
    let parts = input.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }

    let mut values = [0.0; 4];
    for (slot, part) in values.iter_mut().zip(parts) {
        *slot = parse_fraction(part)?;
    }
    let key = values[3];

    Some([
        (1.0 - values[0]) * (1.0 - key),
        (1.0 - values[1]) * (1.0 - key),
        (1.0 - values[2]) * (1.0 - key),
    ])
}

fn named_color(name: &str) -> Option<[f32; 3]> {
    let hex = match name {
        "aliceblue" => "f0f8ff",
        "antiquewhite" => "faebd7",
        "aqua" => "00ffff",
        "aquamarine" => "7fffd4",
        "azure" => "f0ffff",
        "beige" => "f5f5dc",
        "bisque" => "ffe4c4",
        "black" => "000000",
        "blanchedalmond" => "ffebcd",
        "blue" => "0000ff",
        "blueviolet" => "8a2be2",
        "brown" => "a52a2a",
        "burlywood" => "deb887",
        "cadetblue" => "5f9ea0",
        "chartreuse" => "7fff00",
        "chocolate" => "d2691e",
        "coral" => "ff7f50",
        "cornflowerblue" => "6495ed",
        "cornsilk" => "fff8dc",
        "crimson" => "dc143c",
        "cyan" => "00ffff",
        "darkblue" => "00008b",
        "darkcyan" => "008b8b",
        "darkgoldenrod" => "b8860b",
        "darkgray" | "darkgrey" => "a9a9a9",
        "darkgreen" => "006400",
        "darkkhaki" => "bdb76b",
        "darkmagenta" => "8b008b",
        "darkolivegreen" => "556b2f",
        "darkorange" => "ff8c00",
        "darkorchid" => "9932cc",
        "darkred" => "8b0000",
        "darksalmon" => "e9967a",
        "darkseagreen" => "8fbc8f",
        "darkslateblue" => "483d8b",
        "darkslategray" | "darkslategrey" => "2f4f4f",
        "darkturquoise" => "00ced1",
        "darkviolet" => "9400d3",
        "deeppink" => "ff1493",
        "deepskyblue" => "00bfff",
        "dimgray" | "dimgrey" => "696969",
        "dodgerblue" => "1e90ff",
        "firebrick" => "b22222",
        "floralwhite" => "fffaf0",
        "forestgreen" => "228b22",
        "fuchsia" => "ff00ff",
        "gainsboro" => "dcdcdc",
        "ghostwhite" => "f8f8ff",
        "gold" => "ffd700",
        "goldenrod" => "daa520",
        "gray" | "grey" => "808080",
        "green" => "008000",
        "greenyellow" => "adff2f",
        "honeydew" => "f0fff0",
        "hotpink" => "ff69b4",
        "indianred" => "cd5c5c",
        "indigo" => "4b0082",
        "ivory" => "fffff0",
        "khaki" => "f0e68c",
        "lavender" => "e6e6fa",
        "lavenderblush" => "fff0f5",
        "lawngreen" => "7cfc00",
        "lemonchiffon" => "fffacd",
        "lightblue" => "add8e6",
        "lightcoral" => "f08080",
        "lightcyan" => "e0ffff",
        "lightgoldenrodyellow" => "fafad2",
        "lightgray" | "lightgrey" => "d3d3d3",
        "lightgreen" => "90ee90",
        "lightpink" => "ffb6c1",
        "lightsalmon" => "ffa07a",
        "lightseagreen" => "20b2aa",
        "lightskyblue" => "87cefa",
        "lightslategray" | "lightslategrey" => "778899",
        "lightsteelblue" => "b0c4de",
        "lightyellow" => "ffffe0",
        "lime" => "00ff00",
        "limegreen" => "32cd32",
        "linen" => "faf0e6",
        "magenta" => "ff00ff",
        "maroon" => "800000",
        "mediumaquamarine" => "66cdaa",
        "mediumblue" => "0000cd",
        "mediumorchid" => "ba55d3",
        "mediumpurple" => "9370db",
        "mediumseagreen" => "3cb371",
        "mediumslateblue" => "7b68ee",
        "mediumspringgreen" => "00fa9a",
        "mediumturquoise" => "48d1cc",
        "mediumvioletred" => "c71585",
        "midnightblue" => "191970",
        "mintcream" => "f5fffa",
        "mistyrose" => "ffe4e1",
        "moccasin" => "ffe4b5",
        "navajowhite" => "ffdead",
        "navy" => "000080",
        "oldlace" => "fdf5e6",
        "olive" => "808000",
        "olivedrab" => "6b8e23",
        "orange" => "ffa500",
        "orangered" => "ff4500",
        "orchid" => "da70d6",
        "palegoldenrod" => "eee8aa",
        "palegreen" => "98fb98",
        "paleturquoise" => "afeeee",
        "palevioletred" => "db7093",
        "papayawhip" => "ffefd5",
        "peachpuff" => "ffdab9",
        "peru" => "cd853f",
        "pink" => "ffc0cb",
        "plum" => "dda0dd",
        "powderblue" => "b0e0e6",
        "purple" => "800080",
        "red" => "ff0000",
        "rosybrown" => "bc8f8f",
        "royalblue" => "4169e1",
        "saddlebrown" => "8b4513",
        "salmon" => "fa8072",
        "sandybrown" => "f4a460",
        "seagreen" => "2e8b57",
        "seashell" => "fff5ee",
        "sienna" => "a0522d",
        "silver" => "c0c0c0",
        "skyblue" => "87ceeb",
        "slateblue" => "6a5acd",
        "slategray" | "slategrey" => "708090",
        "snow" => "fffafa",
        "springgreen" => "00ff7f",
        "steelblue" => "4682b4",
        "tan" => "d2b48c",
        "teal" => "008080",
        "thistle" => "d8bfd8",
        "tomato" => "ff6347",
        "turquoise" => "40e0d0",
        "violet" => "ee82ee",
        "wheat" => "f5deb3",
        "white" => "ffffff",
        "whitesmoke" => "f5f5f5",
        "yellow" => "ffff00",
        "yellowgreen" => "9acd32",
        _ => return None,
    };

    parse_hex_color(hex)
}

fn specificity(selector: &str) -> u32 {
    fn ident(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
    }

    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut tags = 0u32;
    let mut chars = selector.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '#' => {
                ids += 1;
                while chars.peek().is_some_and(|next| ident(*next)) {
                    chars.next();
                }
            }
            '.' | ':' => {
                classes += 1;
                while chars.peek().is_some_and(|next| ident(*next)) {
                    chars.next();
                }
            }
            '[' => {
                classes += 1;
                for next in chars.by_ref() {
                    if next == ']' {
                        break;
                    }
                }
            }
            c if ident(c) => {
                tags += 1;
                while chars.peek().is_some_and(|next| ident(*next)) {
                    chars.next();
                }
            }
            _ => {}
        }
    }

    ids * 100 + classes * 10 + tags
}

fn strip_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }

    output
}
