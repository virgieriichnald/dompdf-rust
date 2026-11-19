#[test]
fn paginates_long_content() {
    let html = "<p>Baris</p>".repeat(80);
    let pdf = html2pdf_rs::render_html(&html);
    let raw = String::from_utf8_lossy(&pdf);
    let pages = raw.matches("/Type /Page").count() - raw.matches("/Type /Pages").count();

    assert!(pages >= 5, "expected at least 5 pages, got {pages}");
}

#[test]
fn renders_pdf_with_text() {
    let pdf = html2pdf_rs::render_html("<h1>Halo</h1><p>Dunia dompdf</p>");
    assert!(pdf.starts_with(b"%PDF-"));

    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("Halo"));
    assert!(raw.contains("Dunia"));
}

#[test]
fn applies_inline_style() {
    let pdf = html2pdf_rs::render_html(
        "<style>.merah { color: red; }</style><p class=\"merah\">Merah</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("1 0 0 rg"));
    assert!(raw.contains("Merah"));
}

#[test]
fn embeds_data_uri_image() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let pdf = html2pdf_rs::render_html(&format!(
        "<img src=\"data:image/png;base64,{encoded}\" width=\"40\" height=\"20\">"
    ));
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Subtype /Image"));
    assert!(raw.contains("/DeviceRGB"));
    assert!(raw.contains("Im1 Do"));
}

#[test]
fn embeds_local_image() {
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let directory = std::env::temp_dir();
    let path = directory.join(format!("html2pdf-rs-{}.png", std::process::id()));
    let mut png = Vec::new();
    let pixels = [0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 255];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(&path, &png).unwrap();

    let pdf = html2pdf_rs::render_html_with_base(
        &format!(
            "<img src=\"{}\" width=\"30\">",
            path.file_name().unwrap().to_string_lossy()
        ),
        &directory,
    );
    let raw = String::from_utf8_lossy(&pdf);
    std::fs::remove_file(&path).unwrap();

    assert!(raw.contains("/Subtype /Image"));
    assert!(raw.contains("Im1 Do"));
}

#[test]
fn draws_background_and_border() {
    let pdf = html2pdf_rs::render_html(
        "<div style=\"background-color:#ff0000;border:2px solid blue\">Kotak</div>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("Kotak"));
    assert!(raw.contains("1 0 0 rg"));
    assert!(raw.contains("0 0 1 rg"));
    assert!(raw.contains(" re"));
}

#[test]
fn renders_table_with_colspan() {
    let pdf = html2pdf_rs::render_html(
        "<table border=\"1\"><tr><td colspan=\"2\">Atas</td></tr>\
         <tr><td>Kiri</td><td>Kanan</td></tr></table>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("Atas"));
    assert!(raw.contains("Kiri"));
    assert!(raw.contains("Kanan"));
}

#[test]
fn renders_list_items() {
    let pdf = html2pdf_rs::render_html("<ul><li>Satu</li><li>Dua</li></ul>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("Satu"));
    assert!(raw.contains("Dua"));
}

#[test]
fn adds_link_annotation() {
    let pdf = html2pdf_rs::render_html("<a href=\"https://example.com\">Klik</a>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Subtype /Link"));
    assert!(raw.contains("/URI (https://example.com)"));
    assert!(raw.contains("/Annots"));
}

#[test]
fn embeds_inline_svg() {
    let pdf = html2pdf_rs::render_html(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\">\
         <rect width=\"20\" height=\"20\" fill=\"red\"/></svg>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Subtype /Image"));
    assert!(raw.contains("Im1 Do"));
}

#[test]
fn embeds_ttf_font() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/fonts");
    let pdf = html2pdf_rs::render_html_with_base(
        "<style>@font-face{font-family:DejaVu;src:url('DejaVuSans.ttf');}\
         p{font-family:DejaVu}</style><p>Halo Unicode</p>",
        &base,
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Subtype /Type0"));
    assert!(raw.contains("/Identity-H"));
    assert!(raw.contains("/FontFile2"));
    assert!(raw.contains("Tj"));
}
