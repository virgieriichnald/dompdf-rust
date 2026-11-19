use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use scraper::{ElementRef, Selector};

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
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderSpec {
    pub width: f32,
    pub color: [f32; 3],
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
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
}

#[derive(Clone, Debug, Default)]
pub struct PageRule {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: Option<f32>,
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
        match self.unit {
            Unit::Pt => self.value,
            Unit::Px => self.value * 0.75,
            Unit::Em => self.value * font_size,
            Unit::In => self.value * 72.0,
            Unit::Cm => self.value * 72.0 / 2.54,
            Unit::Mm => self.value * 72.0 / 25.4,
            Unit::Percent => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Style {
    pub color: [f32; 3],
    pub font_family: String,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
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
    pub border: Option<BorderSpec>,
    pub text_align: TextAlign,
    pub display: Display,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0],
            font_family: "Helvetica".to_string(),
            font_size: 12.0,
            bold: false,
            italic: false,
            underline: false,
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
            border: None,
            text_align: TextAlign::Left,
            display: Display::Inline,
        }
    }
}

impl Style {
    pub fn inherited(parent: &Style) -> Self {
        Self {
            color: parent.color,
            font_family: parent.font_family.clone(),
            font_size: parent.font_size,
            bold: parent.bold,
            italic: parent.italic,
            underline: parent.underline,
            line_height: parent.line_height,
            ..Self::default()
        }
    }

    pub fn margin_top_pt(&self) -> f32 {
        self.margin_top.resolve(self.font_size)
    }

    pub fn margin_right_pt(&self) -> f32 {
        self.margin_right.resolve(self.font_size)
    }

    pub fn margin_bottom_pt(&self) -> f32 {
        self.margin_bottom.resolve(self.font_size)
    }

    pub fn margin_left_pt(&self) -> f32 {
        self.margin_left.resolve(self.font_size)
    }

    pub fn padding_top_pt(&self) -> f32 {
        self.padding_top.resolve(self.font_size)
    }

    pub fn padding_right_pt(&self) -> f32 {
        self.padding_right.resolve(self.font_size)
    }

    pub fn padding_bottom_pt(&self) -> f32 {
        self.padding_bottom.resolve(self.font_size)
    }

    pub fn padding_left_pt(&self) -> f32 {
        self.padding_left.resolve(self.font_size)
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<(String, String)>,
    pub specificity: u32,
    pub order: usize,
}

pub fn parse_styles(html: &scraper::Html, base: &Path) -> Vec<Rule> {
    let sheets = stylesheets(html, base);
    let mut rules = Vec::new();
    let mut order = 0usize;

    for (text, _) in &sheets {
        rules.extend(parse_css(text, &mut order));
    }

    rules
}

pub fn parse_font_faces(html: &scraper::Html, base: &Path) -> Vec<FontFace> {
    let sheets = stylesheets(html, base);
    let mut faces = Vec::new();

    for (text, sheet_base) in &sheets {
        faces.extend(parse_font_faces_css(text, sheet_base));
    }

    faces
}

pub fn parse_page_rules(html: &scraper::Html, base: &Path) -> PageRule {
    let sheets = stylesheets(html, base);
    let mut page = PageRule::default();

    for (text, _) in &sheets {
        let clean = strip_comments(text);
        let mut rest = clean.as_str();

        while let Some(position) = rest.find("@page") {
            let after = &rest[position + "@page".len()..];
            let Some(open) = after.find('{') else {
                break;
            };
            let selector = after[..open].trim();
            let after_open = &after[open + 1..];
            let Some(close) = after_open.find('}') else {
                break;
            };
            rest = &after_open[close + 1..];

            if !selector.is_empty() {
                continue;
            }

            for (property, value) in parse_declarations(&after_open[..close]) {
                match property.as_str() {
                    "size" => {
                        let (dimensions, landscape) = parse_page_size(&value);
                        if let Some((mut width, mut height)) = dimensions {
                            if landscape {
                                std::mem::swap(&mut width, &mut height);
                            }
                            page.width = Some(width);
                            page.height = Some(height);
                        }
                    }
                    "margin" => {
                        if let Some(length) = parse_length(&value) {
                            page.margin = Some(length.resolve(12.0));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    page
}

fn parse_page_size(value: &str) -> (Option<(f32, f32)>, bool) {
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
                    let points = length.resolve(12.0);
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

fn paper_size(name: &str) -> Option<(f32, f32)> {
    match name {
        "a4" => Some((595.28, 841.89)),
        "letter" => Some((612.0, 792.0)),
        "legal" => Some((612.0, 1008.0)),
        "a3" => Some((841.89, 1190.55)),
        _ => None,
    }
}

fn stylesheets(html: &scraper::Html, base: &Path) -> Vec<(String, PathBuf)> {
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
                if let Some(text) = read_text_resource(href, base) {
                    raw.push((text, resource_base(href, base)));
                }
            }
            _ => {}
        }
    }

    let mut expanded = Vec::new();
    for (text, sheet_base) in raw {
        expand_css(&text, &sheet_base, &mut expanded);
    }
    expanded
}

fn expand_css(input: &str, base: &Path, out: &mut Vec<(String, PathBuf)>) {
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
        if let Some(url) = extract_url(statement)
            && let Some(text) = read_text_resource(&url, base)
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
            expand_css(import, import_base, out);
        }
    }
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

        if let (Some(family), Some(src)) = (family, source) {
            faces.push(FontFace {
                family,
                src,
                base: base.to_path_buf(),
            });
        }
    }

    faces
}

fn read_text_resource(src: &str, base: &Path) -> Option<String> {
    if let Some(position) = src.find(";base64,") {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&src[position + ";base64,".len()..])
            .ok()?;
        return String::from_utf8(bytes).ok();
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        return None;
    }
    fs::read_to_string(base.join(src)).ok()
}

fn resource_base(src: &str, base: &Path) -> PathBuf {
    if src.starts_with("data:") || src.starts_with("http://") || src.starts_with("https://") {
        return base.to_path_buf();
    }
    base.join(src)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.to_path_buf())
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

pub fn parse_css(input: &str, order: &mut usize) -> Vec<Rule> {
    let clean = strip_comments(input);
    let mut rules = Vec::new();
    let mut rest = clean.as_str();

    while let Some(open) = rest.find('{') {
        let selector_text = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            break;
        };
        let body = &after_open[..close];
        rest = &after_open[close + 1..];

        if selector_text.is_empty() || selector_text.starts_with('@') {
            continue;
        }

        let declarations = parse_declarations(body);
        if declarations.is_empty() {
            continue;
        }

        let mut selectors = Vec::new();
        for raw in selector_text.split(',') {
            let text = raw.trim();
            if let Ok(selector) = Selector::parse(text) {
                selectors.push(selector);
            }
        }

        if selectors.is_empty() {
            continue;
        }

        rules.push(Rule {
            selectors,
            declarations,
            specificity: specificity(selector_text),
            order: *order,
        });
        *order += 1;
    }

    rules
}

pub fn parse_declarations(input: &str) -> Vec<(String, String)> {
    input
        .split(';')
        .filter_map(|item| {
            let (property, value) = item.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if property.is_empty() || value.is_empty() {
                None
            } else {
                Some((property, value))
            }
        })
        .collect()
}

pub fn style_for(element: &ElementRef, parent: &Style, rules: &[Rule]) -> Style {
    let tag = element.value().name();
    let mut style = style_for_tag(tag, parent);

    let mut matched = rules
        .iter()
        .filter(|rule| {
            rule.selectors
                .iter()
                .any(|selector| selector.matches(element))
        })
        .collect::<Vec<_>>();
    matched.sort_by_key(|rule| (rule.specificity, rule.order));

    for rule in matched {
        apply_declarations(&mut style, &rule.declarations, parent.font_size);
    }

    if let Some(inline) = element.attr("style") {
        let base_size = style.font_size;
        apply_declarations(&mut style, &parse_declarations(inline), base_size);
    }

    if let Some(align) = element.attr("align") {
        style.text_align = parse_align(align).unwrap_or(style.text_align);
    }

    style
}

pub fn apply_declarations(style: &mut Style, declarations: &[(String, String)], parent_size: f32) {
    for (property, value) in declarations {
        match property.as_str() {
            "color" => {
                if let Some(color) = parse_color(value) {
                    style.color = color;
                }
            }
            "font-size" => {
                if let Some(length) = parse_length(value) {
                    style.font_size = length.resolve_font(parent_size);
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
            "line-height" => {
                if let Ok(value) = value.parse::<f32>() {
                    style.line_height = value;
                }
            }
            "margin" => {
                if let Some(length) = parse_length(value) {
                    style.margin_top = length;
                    style.margin_right = length;
                    style.margin_bottom = length;
                    style.margin_left = length;
                }
            }
            "margin-top" => {
                if let Some(length) = parse_length(value) {
                    style.margin_top = length;
                }
            }
            "margin-right" => {
                if let Some(length) = parse_length(value) {
                    style.margin_right = length;
                }
            }
            "margin-bottom" => {
                if let Some(length) = parse_length(value) {
                    style.margin_bottom = length;
                }
            }
            "margin-left" => {
                if let Some(length) = parse_length(value) {
                    style.margin_left = length;
                }
            }
            "font-family" => {
                if let Some(family) = clean_family(value) {
                    style.font_family = family;
                }
            }
            "text-decoration" | "text-decoration-line" => {
                style.underline = value.to_ascii_lowercase().contains("underline");
            }
            "background-color" => style.background = parse_color(value),
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
            }
            "padding" => {
                if let Some(length) = parse_length(value) {
                    style.padding_top = length;
                    style.padding_right = length;
                    style.padding_bottom = length;
                    style.padding_left = length;
                }
            }
            "padding-top" => {
                if let Some(length) = parse_length(value) {
                    style.padding_top = length;
                }
            }
            "padding-right" => {
                if let Some(length) = parse_length(value) {
                    style.padding_right = length;
                }
            }
            "padding-bottom" => {
                if let Some(length) = parse_length(value) {
                    style.padding_bottom = length;
                }
            }
            "padding-left" => {
                if let Some(length) = parse_length(value) {
                    style.padding_left = length;
                }
            }
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                let sides = match property.as_str() {
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
                } else if let Some(spec) = parse_border(value, sides, style.font_size) {
                    style.border = Some(spec);
                }
            }
            "text-align" => {
                if let Some(align) = parse_align(value) {
                    style.text_align = align;
                }
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
}

impl Length {
    fn resolve_font(self, parent_size: f32) -> f32 {
        match self.unit {
            Unit::Pt => self.value,
            Unit::Px => self.value * 0.75,
            Unit::Em => self.value * parent_size,
            Unit::In => self.value * 72.0,
            Unit::Cm => self.value * 72.0 / 2.54,
            Unit::Mm => self.value * 72.0 / 25.4,
            Unit::Percent => parent_size * self.value / 100.0,
        }
    }
}

fn style_for_tag(tag: &str, parent: &Style) -> Style {
    let mut style = Style::inherited(parent);

    match tag {
        "html" | "body" | "div" | "section" | "article" | "header" | "footer" | "main"
        | "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" | "ul" | "ol" | "li" => {
            style.display = Display::Block;
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

fn parse_length(input: &str) -> Option<Length> {
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

fn parse_border(input: &str, sides: [bool; 4], font_size: f32) -> Option<BorderSpec> {
    let mut width = None;
    let mut color = None;

    for token in input.split_whitespace() {
        if width.is_none()
            && let Some(length) = parse_length(token)
        {
            width = Some(length.resolve(font_size));
            continue;
        }
        if color.is_none()
            && let Some(parsed) = parse_color(token)
        {
            color = Some(parsed);
        }
    }

    Some(BorderSpec {
        width: width.unwrap_or(1.0),
        color: color.unwrap_or([0.0, 0.0, 0.0]),
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
        _ => None,
    }
}

fn parse_color(input: &str) -> Option<[f32; 3]> {
    let text = input.trim().to_ascii_lowercase();

    if let Some(hex) = text.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = hex.get(0..1)?.parse::<u8>().ok()?;
                let g = hex.get(1..2)?.parse::<u8>().ok()?;
                let b = hex.get(2..3)?.parse::<u8>().ok()?;
                Some([
                    r as f32 * 17.0 / 255.0,
                    g as f32 * 17.0 / 255.0,
                    b as f32 * 17.0 / 255.0,
                ])
            }
            6 => {
                let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
                let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
                let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
                Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
            }
            _ => None,
        };
    }

    if let Some(inner) = text.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts = inner
            .split(',')
            .map(|part| part.trim().trim_end_matches('%').parse::<f32>().ok())
            .collect::<Option<Vec<_>>>()?;
        if parts.len() == 3 {
            let scale = if inner.contains('%') { 100.0 } else { 255.0 };
            return Some([parts[0] / scale, parts[1] / scale, parts[2] / scale]);
        }
    }

    let named = match text.as_str() {
        "black" => [0.0, 0.0, 0.0],
        "white" => [1.0, 1.0, 1.0],
        "red" => [1.0, 0.0, 0.0],
        "green" => [0.0, 0.5, 0.0],
        "blue" => [0.0, 0.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0],
        "orange" => [1.0, 0.65, 0.0],
        "purple" => [0.5, 0.0, 0.5],
        "teal" => [0.0, 0.5, 0.5],
        "navy" => [0.0, 0.0, 0.5],
        "gray" | "grey" => [0.5, 0.5, 0.5],
        "silver" => [0.75, 0.75, 0.75],
        _ => return None,
    };

    Some(named)
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
