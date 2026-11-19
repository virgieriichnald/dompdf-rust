#[test]
fn writes_pdf_metadata() {
    let pdf = html2pdf_rs::render_html(
        "<title>Judul Dokumen</title>\
         <meta name=\"author\" content=\"Budi\">\
         <meta name=\"description\" content=\"Ringkasan\">\
         <p>Isi</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Title (Judul Dokumen)"));
    assert!(raw.contains("/Author (Budi)"));
    assert!(raw.contains("/Subject (Ringkasan)"));
}

#[test]
fn loads_external_css_and_imports() {
    use std::fs;

    let directory = std::env::temp_dir().join(format!("html2pdf-rs-css-{}", std::process::id()));
    let css_directory = directory.join("css");
    fs::create_dir_all(&css_directory).unwrap();
    fs::write(
        css_directory.join("main.css"),
        "@import url('import.css'); p { color: green; }",
    )
    .unwrap();
    fs::write(css_directory.join("import.css"), "h1 { color: red; }").unwrap();

    let pdf = html2pdf_rs::render_html_with_base(
        "<link rel=\"stylesheet\" href=\"css/main.css\"><h1>Judul</h1><p>Paragraf</p>",
        &directory,
    );
    let raw = String::from_utf8_lossy(&pdf);
    fs::remove_dir_all(&directory).unwrap();

    assert!(raw.contains("Judul"));
    assert!(raw.contains("Paragraf"));
    assert!(raw.contains("1 0 0 rg"));
    assert!(raw.contains("0 0.5 0 rg"));
}

#[test]
fn applies_page_at_rule() {
    let pdf = html2pdf_rs::render_html(
        "<style>@page { size: letter landscape; margin: 1cm; }</style><p>Halaman</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("792 612"));
}

#[test]
fn resolves_base_href_and_image_link() {
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
    use std::fs;

    let directory = std::env::temp_dir().join(format!("html2pdf-rs-base-{}", std::process::id()));
    let sub = directory.join("sub");
    fs::create_dir_all(&sub).unwrap();

    let mut png = Vec::new();
    let pixels = [0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    fs::write(sub.join("g.png"), &png).unwrap();

    let pdf = html2pdf_rs::render_html_with_base(
        "<base href=\"sub/\"><a href=\"next.html\"><img src=\"g.png\" width=\"30\"></a>",
        &directory,
    );
    let raw = String::from_utf8_lossy(&pdf);
    fs::remove_dir_all(&directory).unwrap();

    assert!(raw.contains("/Subtype /Image"));
    assert!(raw.contains("Im1 Do"));
    assert!(raw.contains("/Subtype /Link"));
    assert!(raw.contains("next.html"));
}

#[test]
fn applies_page_options() {
    use html2pdf_rs::Options;
    use std::path::Path;

    let options = Options::paper("letter").unwrap().landscape().margin(20.0);
    let pdf = html2pdf_rs::render_html_with_options("<p>Halaman</p>", Path::new("."), &options);
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("792 612"));
}

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
