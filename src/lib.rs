use std::path::Path;

pub mod css;
pub mod layout;
pub mod pdf;

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub width: f32,
    pub height: f32,
    pub margin: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 595.28,
            height: 841.89,
            margin: 54.0,
        }
    }
}

impl Options {
    pub fn paper(name: &str) -> Option<Self> {
        let (width, height) = match name.to_ascii_lowercase().as_str() {
            "a4" => (595.28, 841.89),
            "letter" => (612.0, 792.0),
            "legal" => (612.0, 1008.0),
            "a3" => (841.89, 1190.55),
            _ => return None,
        };
        Some(Self {
            width,
            height,
            ..Self::default()
        })
    }

    pub fn landscape(mut self) -> Self {
        std::mem::swap(&mut self.width, &mut self.height);
        self
    }

    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin.max(0.0);
        self
    }
}

pub fn render_html(html: &str) -> Vec<u8> {
    render_html_with_base(html, Path::new("."))
}

pub fn render_html_with_base(html: &str, base: &Path) -> Vec<u8> {
    render_html_with_options(html, base, &Options::default())
}

pub fn render_html_with_options(html: &str, base: &Path, options: &Options) -> Vec<u8> {
    let document = layout::Document::from_html_with_options(html, base, options);
    pdf::write_pdf(&document)
}
