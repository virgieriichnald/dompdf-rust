use std::path::Path;

pub mod css;
pub mod layout;
pub mod pdf;

pub fn render_html(html: &str) -> Vec<u8> {
    render_html_with_base(html, Path::new("."))
}

pub fn render_html_with_base(html: &str, base: &Path) -> Vec<u8> {
    let document = layout::Document::from_html_with_base(html, base);
    pdf::write_pdf(&document)
}
