use std::path::Path;

use crate::css::paper_size;

pub mod css;
pub mod layout;
pub mod pdf;

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub width: f32,
    pub height: f32,
    pub margin: f32,
    pub default_font: String,
    pub dpi: f32,
    pub image_byte_limit: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 595.28,
            height: 841.89,
            margin: 54.0,
            default_font: "Helvetica".to_string(),
            dpi: crate::css::DEFAULT_DPI,
            image_byte_limit: None,
        }
    }
}

impl Options {
    pub fn paper(name: &str) -> Option<Self> {
        let (width, height) = paper_size(name)?;
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

    pub fn font(mut self, family: &str) -> Self {
        if !family.trim().is_empty() {
            self.default_font = family.trim().to_string();
        }
        self
    }

    pub fn dpi(mut self, dpi: f32) -> Self {
        self.dpi = dpi.max(1.0);
        self
    }

    pub fn image_byte_limit(mut self, limit: usize) -> Self {
        self.image_byte_limit = Some(limit);
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
