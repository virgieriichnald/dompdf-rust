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
    assert!(raw.contains("0 0.5019608 0 rg"));
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
fn parses_extended_color_syntax() {
    use html2pdf_rs::css::{Style, apply_declarations, parse_declarations};

    let mut style = Style::default();
    apply_declarations(
        &mut style,
        &parse_declarations("color: rgb(0 0 255 / 50%); background-color: hsl(0, 100%, 50%)"),
        12.0,
    );

    assert_eq!(style.color, [0.0, 0.0, 1.0]);
    assert_eq!(style.background, Some([1.0, 0.0, 0.0]));

    apply_declarations(
        &mut style,
        &parse_declarations("color: cmyk(0,1,1,0)"),
        12.0,
    );
    assert_eq!(style.color, [1.0, 0.0, 0.0]);
}

#[test]
fn applies_box_shorthand_and_unitless_line_height() {
    use html2pdf_rs::css::{Style, apply_declarations, parse_declarations};

    let mut style = Style::default();
    apply_declarations(
        &mut style,
        &parse_declarations(
            "margin: 1pt 2pt 3pt 4pt; padding: 5pt 6pt; font-size: large; line-height: 150%",
        ),
        12.0,
    );

    assert_eq!(style.margin_top_pt(), 1.0);
    assert_eq!(style.margin_right_pt(), 2.0);
    assert_eq!(style.margin_bottom_pt(), 3.0);
    assert_eq!(style.margin_left_pt(), 4.0);
    assert_eq!(style.padding_top_pt(), 5.0);
    assert_eq!(style.padding_right_pt(), 6.0);
    assert_eq!(style.padding_bottom_pt(), 5.0);
    assert_eq!(style.padding_left_pt(), 6.0);
    assert!((style.font_size - 14.4).abs() < 0.001);
    assert!((style.line_height - 1.5).abs() < 0.001);
}

#[test]
fn resolves_extended_paper_sizes_and_dpi() {
    use html2pdf_rs::Options;
    use html2pdf_rs::css::{Length, Unit, paper_size, px_scale};

    assert_eq!(paper_size("B5"), Some((498.90, 708.66)));
    assert_eq!(paper_size("commercial #10 envelope"), Some((684.0, 297.0)));

    let options = Options::paper("b5").unwrap().dpi(192.0);
    assert_eq!((options.width, options.height), (498.90, 708.66));
    assert!((px_scale(192.0) - 0.375).abs() < 0.0001);
    assert_eq!(
        Length {
            value: 192.0,
            unit: Unit::Px
        }
        .resolve_with(12.0, px_scale(192.0)),
        72.0
    );
}

#[test]
fn rejects_images_over_byte_limit() {
    use base64::Engine;
    use html2pdf_rs::Options;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
    use std::path::Path;

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let html = format!("<img src=\"data:image/png;base64,{encoded}\">");

    let rejected = html2pdf_rs::render_html_with_options(
        &html,
        Path::new("."),
        &Options::default().image_byte_limit(4),
    );
    let accepted = html2pdf_rs::render_html_with_options(
        &html,
        Path::new("."),
        &Options::default().image_byte_limit(png.len()),
    );

    assert!(!String::from_utf8_lossy(&rejected).contains("/Subtype /Image"));
    assert!(String::from_utf8_lossy(&accepted).contains("/Subtype /Image"));
}

#[test]
fn embeds_bmp_image() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::bmp::BmpEncoder};

    let mut bmp = Vec::new();
    let pixels = [0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 255];
    BmpEncoder::new(&mut bmp)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp);

    let pdf = html2pdf_rs::render_html(&format!(
        "<img src=\"data:image/bmp;base64,{encoded}\" width=\"30\">"
    ));

    assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
}

#[test]
fn applies_presentational_attributes_with_css_precedence() {
    let pdf = html2pdf_rs::render_html("<p bgcolor=\"#ff0000\" align=\"center\">Teks</p>");
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("1 0 0 rg"));

    let pdf = html2pdf_rs::render_html(
        "<style>p{background-color:blue}</style><p bgcolor=\"red\">Teks</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("0 0 1 rg"));
    assert!(!raw.contains("1 0 0 rg"));

    let pdf =
        html2pdf_rs::render_html("<p bgcolor=\"red\" style=\"background-color:green\">Teks</p>");
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("0 0.5019608 0 rg"));
    assert!(!raw.contains("1 0 0 rg"));
}

#[test]
fn renders_ordered_list_markers() {
    let pdf = html2pdf_rs::render_html(
        "<ol start=\"3\" type=\"A\"><li>Satu</li><li>Dua</li></ol>\
         <ol reversed start=\"2\"><li>Tiga</li></ol>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(C.)"));
    assert!(raw.contains("(D.)"));
    assert!(raw.contains("(2.)"));
}

#[test]
fn parses_important_and_text_features() {
    use html2pdf_rs::css::{Style, TextAlign, apply_declarations, parse_declarations};

    let mut style = Style::default();
    apply_declarations(
        &mut style,
        &parse_declarations(
            "text-align:justify;text-indent:1cm;letter-spacing:2pt;white-space:pre",
        ),
        12.0,
    );
    assert_eq!(style.text_align, TextAlign::Justify);
    assert!((style.text_indent_pt() - 28.34645).abs() < 0.01);
    assert!((style.letter_spacing_pt() - 2.0).abs() < 0.001);
    assert_eq!(style.white_space, html2pdf_rs::css::WhiteSpace::Pre);

    let pdf = html2pdf_rs::render_html(
        "<style>p{color:red !important} p{color:blue}</style><p style=\"color:green\">X</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("1 0 0 rg"));
    assert!(!raw.contains("0 0 1 rg"));
    assert!(!raw.contains("0 0.5019608 0 rg"));
}

#[test]
fn preserves_preformatted_whitespace_and_letter_spacing() {
    let pdf =
        html2pdf_rs::render_html("<p style=\"white-space:pre;letter-spacing:2pt\">a  b\nc</p>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.matches("( )").count() >= 2);
    assert!(raw.contains("2 Tc"));
}

#[test]
fn embeds_bold_face_for_bold_text() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/fonts");
    let pdf = html2pdf_rs::render_html_with_base(
        "<style>@font-face{font-family:F;src:url('DejaVuSans.ttf');}\
         @font-face{font-family:F;src:url('DejaVuSans-Bold.ttf');font-weight:bold;}\
         p{font-family:F}</style><p>A <strong>B</strong></p>",
        &base,
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert_eq!(raw.matches("/FontFile2").count(), 2);
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

fn encode_woff2_base128(value: u32, output: &mut Vec<u8>) {
    let mut value = value;
    let mut bytes = Vec::new();
    loop {
        bytes.push((value & 0x7f) as u8);
        value >>= 7;
        if value == 0 {
            break;
        }
    }
    bytes.reverse();
    let count = bytes.len();
    for (index, byte) in bytes.into_iter().enumerate() {
        output.push(if index + 1 == count {
            byte
        } else {
            byte | 0x80
        });
    }
}

fn make_test_woff2(ttf: &[u8]) -> Vec<u8> {
    fn u16be(bytes: &[u8], offset: usize) -> usize {
        u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize
    }
    fn u32be(bytes: &[u8], offset: usize) -> usize {
        u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize
    }
    fn known_index(tag: &[u8; 4]) -> Option<u8> {
        let tags = [
            b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ",
            b"fpgm", b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp",
            b"hdmx", b"kern", b"LTSH", b"PCLT", b"VDMX", b"vhea", b"vmtx", b"BASE", b"GDEF",
            b"GPOS", b"GSUB", b"EBSC", b"JSTF", b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL",
            b"SVG ", b"sbix", b"acnt", b"avar", b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc",
            b"feat", b"fmtx", b"fvar", b"gvar", b"hsty", b"just", b"lcar", b"mort", b"morx",
            b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat", b"Gloc", b"Feat", b"Sill",
        ];
        tags.iter()
            .position(|candidate| **candidate == *tag)
            .map(|index| index as u8)
    }

    let num_tables = u16be(ttf, 4);
    let mut directory = Vec::new();
    let mut font_data = Vec::new();
    for index in 0..num_tables {
        let record = 12 + index * 16;
        let tag = &ttf[record..record + 4];
        let tag = <&[u8; 4]>::try_from(tag).unwrap();
        let offset = u32be(ttf, record + 8);
        let length = u32be(ttf, record + 12);
        let data = &ttf[offset..offset + length];
        let known = known_index(tag);
        let mut flags = known.unwrap_or(63);
        if tag == b"glyf" || tag == b"loca" {
            flags |= 3 << 6;
        }
        if known.is_none() {
            directory.push(flags);
            directory.extend_from_slice(tag);
        } else {
            directory.push(flags);
        }
        encode_woff2_base128(length as u32, &mut directory);
        font_data.extend_from_slice(data);
    }

    let mut compressed = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 11, 22);
        std::io::Write::write_all(&mut writer, &font_data).unwrap();
    }
    let mut output = Vec::new();
    output.extend_from_slice(b"wOF2");
    output.extend_from_slice(&ttf[0..4]);
    let total_len = 48 + directory.len() + compressed.len();
    output.extend_from_slice(&(total_len as u32).to_be_bytes());
    output.extend_from_slice(&(num_tables as u16).to_be_bytes());
    output.extend_from_slice(&0u16.to_be_bytes());
    output.extend_from_slice(&(ttf.len() as u32).to_be_bytes());
    output.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    output.extend_from_slice(&1u16.to_be_bytes());
    output.extend_from_slice(&0u16.to_be_bytes());
    output.extend_from_slice(&[0u8; 20]);
    output.extend_from_slice(&directory);
    output.extend_from_slice(&compressed);
    output
}

#[test]
fn embeds_woff2_font() {
    use std::fs;

    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/fonts/DejaVuSans.ttf");
    let ttf = fs::read(source).unwrap();
    let woff2 = make_test_woff2(&ttf);
    let directory = std::env::temp_dir().join(format!("html2pdf-rs-woff2-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("DejaVuSans.woff2");
    fs::write(&path, &woff2).unwrap();

    let pdf = html2pdf_rs::render_html_with_base(
        "<style>@font-face{font-family:DejaVu;src:url('DejaVuSans.woff2');}\
         p{font-family:DejaVu}</style><p>Halo WOFF2</p>",
        &directory,
    );
    fs::remove_dir_all(&directory).unwrap();
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/FontFile2"), "missing embedded font");
    assert!(raw.contains("Tj"), "missing text");
}

#[test]
fn embeds_production_woff2_font() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let pdf = html2pdf_rs::render_html_with_base(
        "<style>@font-face{font-family:DejaVu;src:url('DejaVuSans.woff2');}\
         p{font-family:DejaVu}</style><p>Halo WOFF2 produksi</p>",
        &base,
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/FontFile2"), "missing embedded font");
    assert!(raw.contains("Tj"), "missing text");
}

#[test]
fn applies_media_rules_and_conditional_imports() {
    use std::fs;

    let pdf = html2pdf_rs::render_html(
        "<style>@media screen { p { color: blue } }\
         @media print and (min-width: 100px) { p { color: red } }</style><p>Media</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("1 0 0 rg"));
    assert!(!raw.contains("0 0 1 rg"));

    let directory = std::env::temp_dir().join(format!("html2pdf-rs-media-{}", std::process::id()));
    let css = directory.join("css");
    fs::create_dir_all(&css).unwrap();
    fs::write(
        css.join("main.css"),
        "@import url('print.css') print; @import url('screen.css') screen;",
    )
    .unwrap();
    fs::write(css.join("print.css"), "p { color: green }").unwrap();
    fs::write(css.join("screen.css"), "p { color: blue }").unwrap();

    let pdf = html2pdf_rs::render_html_with_base(
        "<link rel=\"stylesheet\" href=\"css/main.css\"><p>Impor</p>",
        &directory,
    );
    let raw = String::from_utf8_lossy(&pdf);
    fs::remove_dir_all(&directory).unwrap();

    assert!(raw.contains("0 0.5019608 0 rg"));
    assert!(!raw.contains("0 0 1 rg"));
}

#[test]
fn applies_page_pseudo_and_named_rules() {
    let pdf = html2pdf_rs::render_html(
        "<style>@page { size: 200pt 300pt } @page :first { size: 100pt 150pt }</style>\
         <p>Pertama</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("100 150"));

    let pdf = html2pdf_rs::render_html(
        "<style>@page cover { size: 300pt 400pt } body { page: cover }</style><p>Sampul</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("300 400"));
}

#[test]
fn restricts_local_resources_with_chroot() {
    use html2pdf_rs::{ResourcePolicy, read_bytes};
    use std::fs;

    let directory = std::env::temp_dir().join(format!("html2pdf-rs-chroot-{}", std::process::id()));
    let root = directory.join("root");
    let outside = directory.join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("allowed.txt"), "allowed").unwrap();
    fs::write(outside.join("secret.txt"), "secret").unwrap();

    let policy = ResourcePolicy::default().chroot(&root.to_string_lossy());
    let allowed = read_bytes("allowed.txt", &root, &policy);
    let blocked = read_bytes("../outside/secret.txt", &root, &policy);
    fs::remove_dir_all(&directory).unwrap();

    assert_eq!(allowed.as_deref(), Some(b"allowed".as_slice()));
    assert!(blocked.is_none());
}

#[test]
fn resolves_css_variables_and_calc() {
    let pdf = html2pdf_rs::render_html(
        "<style>:root { --warna: red; --ukuran: 20pt }\
         p { color: var(--warna); font-size: calc(var(--ukuran) + 4pt) }</style><p>Variabel</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("1 0 0 rg"));
    assert!(raw.contains("24 Tf"));
}

#[test]
fn renders_text_decorations() {
    let pdf = html2pdf_rs::render_html(
        "<style>p { text-decoration: line-through overline }</style><p>Teks</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(Teks) Tj"));
    assert!(raw.matches(" l\nS").count() >= 2);
}

#[test]
fn writes_internal_anchor_destinations() {
    let pdf =
        html2pdf_rs::render_html("<a href=\"#tujuan\">Ke tujuan</a><h2 id=\"tujuan\">Tujuan</h2>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Dests"));
    assert!(raw.contains("/GoTo"));
    assert!(raw.contains("tujuan"));
    assert!(raw.contains("/XYZ"));
}

#[test]
fn overlays_page_text_with_tokens() {
    use html2pdf_rs::{Options, PageTextLocation};
    use std::path::Path;

    let options = Options::default()
        .page_text("Hal {PAGE_NUM}/{PAGE_COUNT}")
        .page_text_location(PageTextLocation::BottomRight);
    let pdf = html2pdf_rs::render_html_with_options("<p>Isi</p>", Path::new("."), &options);
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(Hal 1/1)"));
}

#[test]
fn renders_pseudo_element_generated_content() {
    let pdf = html2pdf_rs::render_html(
        "<style>.item::before { content: '>> ' }\
         .item::after { content: attr(data-suffix) }</style>\
         <p class=\"item\" data-suffix=\" <<\">Tengah</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(>>)"));
    assert!(raw.contains("(Tengah)"));
    assert!(raw.contains("(<<)"));
}

#[test]
fn applies_relative_positioning_offsets() {
    let pdf =
        html2pdf_rs::render_html("<div style=\"position:relative;left:40pt;top:10pt\">Geser</div>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("1 0 0 1 94 "));
}

fn text_matrices(raw: &str) -> Vec<(f32, f32)> {
    raw.match_indices("1 0 0 1 ")
        .filter_map(|(index, _)| {
            let rest = &raw[index + "1 0 0 1 ".len()..];
            let end = rest.find(" Tm")?;
            let values = rest[..end]
                .split_whitespace()
                .filter_map(|value| value.parse::<f32>().ok())
                .collect::<Vec<_>>();
            if values.len() >= 2 {
                Some((values[0], values[1]))
            } else {
                None
            }
        })
        .collect()
}

fn background_image_transforms(raw: &str) -> Vec<[f32; 6]> {
    raw.lines()
        .filter(|line| line.trim_end().ends_with(" cm"))
        .filter_map(|line| {
            let values = line
                .trim()
                .trim_end_matches(" cm")
                .split_whitespace()
                .filter_map(|value| value.parse::<f32>().ok())
                .collect::<Vec<_>>();
            values.try_into().ok()
        })
        .filter(|values: &[f32; 6]| (values[0] - 20.0).abs() < 0.01)
        .collect()
}

fn image_draw_count(raw: &str) -> usize {
    raw.matches("/Im1 Do").count()
}

#[test]
fn encrypts_pdf_with_standard_security() {
    let options = html2pdf_rs::Options::default().encrypt("secret");
    let pdf = html2pdf_rs::render_html_with_options(
        "<p>Halaman terenkripsi</p>",
        std::path::Path::new("."),
        &options,
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Encrypt "), "missing Encrypt entry");
    assert!(raw.contains("/Filter /Standard"), "missing standard filter");
    assert!(raw.contains("/ID [<"), "missing file ID");
    assert!(raw.contains("startxref"), "missing xref");
}

#[test]
fn encrypts_pdf_with_aes_128() {
    let options = html2pdf_rs::Options::default().encrypt_aes("secret");
    let pdf = html2pdf_rs::render_html_with_options(
        "<p>Halaman terenkripsi AES</p>",
        std::path::Path::new("."),
        &options,
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/V 4"), "missing AES revision");
    assert!(raw.contains("/R 4"), "missing security revision 4");
    assert!(raw.contains("/AESV2"), "missing AESV2 crypt filter");
    assert!(raw.contains("/Length 128"), "wrong key length");
    assert!(
        raw.contains("/EncryptMetadata true"),
        "missing metadata flag"
    );
}

#[test]
fn writes_pdf_a_output_intent_and_metadata() {
    let options = html2pdf_rs::Options::default().pdf_a();
    let pdf = html2pdf_rs::render_html_with_options(
        "<html lang=\"id\"><head><title>PDF A</title></head><body><p>Halo</p></body></html>",
        std::path::Path::new("."),
        &options,
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.starts_with("%PDF-1.4"), "wrong PDF version");
    assert!(raw.contains("/GTS_PDFA1"), "missing PDF/A output intent");
    assert!(
        raw.contains("/DestOutputProfile"),
        "missing ICC output profile"
    );
    assert!(raw.contains("/N 3"), "wrong ICC profile color space");
    assert!(
        raw.contains("/OutputIntents ["),
        "missing catalog output intents"
    );
    assert!(raw.contains("/Type /Metadata"), "missing XMP metadata");
    assert!(raw.contains("/Lang (id)"), "missing document language");
    assert_eq!(
        raw.matches("/Lang (id)").count(),
        1,
        "duplicate language entry"
    );
}

#[test]
fn renders_background_image_with_opacity_and_radius() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let pdf = html2pdf_rs::render_html(&format!(
        "<style>div {{ background-image: url(data:image/png;base64,{encoded}); \
         background-repeat: no-repeat; opacity: 0.5; border-radius: 6pt }}</style>\
         <div>Latar</div>"
    ));
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/ExtGState"));
    assert!(raw.contains("/GS1"));
    assert!(raw.contains(" gs"));
    assert!(raw.contains("Im1 Do"));
}

#[test]
fn renders_absolute_overlay_with_z_index() {
    let pdf = html2pdf_rs::render_html(
        "<style>.box { position: absolute; left: 120pt; top: 30pt; z-index: 5 }</style>\
         <p>Isi</p><div class=\"box\">Overlay</div>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("1 0 0 1 174 "));
    assert!(raw.find("(Isi)").unwrap() < raw.find("(Overlay)").unwrap());
}

#[test]
fn resolves_css_counters_in_generated_content() {
    let pdf = html2pdf_rs::render_html(
        "<style>body { counter-reset: n }\
         h1::before { counter-increment: n; content: '[' counter(n) ']' }</style><h1>X</h1>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("([1])"));
}

#[test]
fn sizes_table_columns_from_content() {
    let pdf = html2pdf_rs::render_html("<table><tr><td>A</td><td>BBBBBBBBBB</td></tr></table>");
    let raw = String::from_utf8_lossy(&pdf);
    let matrices = text_matrices(&raw);

    assert!(matrices.len() >= 2);
    assert!(matrices[1].0 - matrices[0].0 > 40.0);
    assert!(matrices[1].0 - matrices[0].0 < 150.0);
}

#[test]
fn aligns_table_cell_content_to_bottom() {
    let pdf = html2pdf_rs::render_html(
        "<style>td { vertical-align: bottom }</style>\
         <table><tr><td>A</td><td>B<br>C<br>D</td></tr></table>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    let matrices = text_matrices(&raw);

    assert!(matrices.len() >= 4);
    assert!((matrices[0].1 - matrices.last().unwrap().1).abs() < 0.2);
}

#[test]
fn repeats_table_header_across_pages() {
    let rows = (1..=60)
        .map(|index| format!("<tr><td>R{index}</td></tr>"))
        .collect::<String>();
    let pdf = html2pdf_rs::render_html(&format!(
        "<table><thead><tr><th>Header</th></tr></thead><tbody>{rows}</tbody></table>"
    ));
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.matches("(Header)").count() >= 2);
}

#[test]
fn applies_rgba_alpha_to_text_background_and_border() {
    let pdf = html2pdf_rs::render_html(
        "<style>p { color: rgba(255,0,0,0.5); background-color: rgba(0,0,255,0.25); \
         border: 1pt solid rgba(0,128,0,0.75) }</style><p>Alfa</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/ca 0.5"));
    assert!(raw.contains("/ca 0.25"));
    assert!(raw.contains("/ca 0.75"));
}

#[test]
fn wraps_text_around_floats() {
    let pdf = html2pdf_rs::render_html(
        "<div style='float:left;border:1pt solid black;padding:4pt'>FLOATING</div>\
         <p>AAAA BBBB CCCC DDDD EEEE FFFF GGGG HHHH</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    let matrices = text_matrices(&raw);
    let first_text_x = matrices.first().map(|(x, _)| *x).unwrap_or(0.0);

    assert!(first_text_x > 100.0, "x={first_text_x}");
}

#[test]
fn positions_float_and_respects_clear() {
    let pdf = html2pdf_rs::render_html(
        "<style>.f { float: left; border: 1pt solid black } \
         .c { clear: both }</style><div class=\"f\">F</div><p class=\"c\">Setelah</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);
    let matrices = text_matrices(&raw);

    assert!(matrices.len() >= 2);
    assert!(matrices[1].1 > matrices[0].1);
}

#[test]
fn stacks_floats_on_the_same_side() {
    let render = |floats: &str| {
        let pdf = html2pdf_rs::render_html(&format!(
            "<style>.f {{ float: left; border: 1pt solid black }} \
             .c {{ clear: left }}</style>{floats}<p class=\"c\">Setelah</p>"
        ));
        let raw = String::from_utf8_lossy(&pdf);
        text_matrices(&raw)
            .into_iter()
            .map(|matrix| matrix.1)
            .fold(f32::INFINITY, f32::min)
    };

    let one = render("<div class=\"f\">Satu</div>");
    let two = render("<div class=\"f\">Satu</div><div class=\"f\">Dua</div>");
    assert!(two < one - 10.0);
}

#[test]
fn floats_respect_horizontal_margins() {
    let render = |margin: u32| {
        let pdf = html2pdf_rs::render_html(&format!(
            "<style>.f {{ float: left; margin-right: {margin}pt }}</style>\
             <div class=\"f\">F</div><p>Teks</p>"
        ));
        text_matrices(&String::from_utf8_lossy(&pdf))
            .into_iter()
            .map(|matrix| matrix.0)
            .fold(0.0f32, f32::max)
    };

    let narrow = render(0);
    let wide = render(80);
    assert!(wide > narrow + 60.0, "narrow={narrow} wide={wide}");
}

#[test]
fn clear_respects_float_bottom_margin() {
    let render = |margin: u32| {
        let pdf = html2pdf_rs::render_html(&format!(
            "<style>.f {{ float: left; margin-bottom: {margin}pt }} \
             .c {{ clear: left }}</style><div class=\"f\">F</div><p class=\"c\">Setelah</p>"
        ));
        text_matrices(&String::from_utf8_lossy(&pdf))
            .into_iter()
            .map(|matrix| matrix.1)
            .fold(f32::INFINITY, f32::min)
    };

    let tight = render(0);
    let roomy = render(60);
    assert!(roomy < tight - 45.0, "tight={tight} roomy={roomy}");
}

#[test]
fn absolute_overlay_uses_full_available_width() {
    let text = "A".repeat(90);
    let pdf = html2pdf_rs::render_html(&format!(
        "<div style=\"position:absolute;left:0;top:0\">{text}</div>"
    ));
    let matrices = text_matrices(&String::from_utf8_lossy(&pdf));

    assert_eq!(matrices.len(), 1, "matrices={matrices:?}");
}

#[test]
fn renders_nested_counters() {
    let pdf = html2pdf_rs::render_html(
        "<style>section { counter-reset: bab }\
         section::before { counter-increment: bab; content: counters(bab, '.') ' ' }</style>\
         <section>Luar<section>Dalam</section></section>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(1)"));
    assert!(raw.contains("(1.1)"));
}

#[test]
fn resolves_calc_percent_against_containing_width() {
    let plain = html2pdf_rs::render_html("<p style=\"margin-top:1pt\">Atas</p>");
    let calculated = html2pdf_rs::render_html("<p style=\"margin-top:calc(10% + 1pt)\">Atas</p>");
    let plain_y = text_matrices(&String::from_utf8_lossy(&plain))[0].1;
    let calculated_y = text_matrices(&String::from_utf8_lossy(&calculated))[0].1;
    let shift = plain_y - calculated_y;

    assert!(shift > 40.0 && shift < 60.0);
}

#[test]
fn renders_individual_border_radius_corners() {
    let uniform = html2pdf_rs::render_html(
        "<p style=\"border:1pt solid black;border-radius:2pt\">RRRRRRRRRRRRRRRRRRRRRRRRRRRR</p>",
    );
    let corners = html2pdf_rs::render_html(
        "<p style=\"border:1pt solid black;border-radius:2pt 4pt 6pt 8pt\">RRRRRRRRRRRRRRRRRRRRRRRRRRRR</p>",
    );
    let raw = String::from_utf8_lossy(&corners);

    assert_ne!(uniform, corners);
    assert!(raw.contains(" c"));
}

#[test]
fn positions_background_image_from_right_top() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let pdf = html2pdf_rs::render_html(&format!(
        "<style>div {{ background-image: url(data:image/png;base64,{encoded}); \
         background-repeat: no-repeat; background-size: 20pt 20pt; \
         background-position: right top }}</style><div>Latar</div>"
    ));
    let raw = String::from_utf8_lossy(&pdf);
    let transform_x = raw
        .lines()
        .filter(|line| line.trim_end().ends_with(" cm"))
        .filter_map(|line| {
            let values = line
                .trim()
                .trim_end_matches(" cm")
                .split_whitespace()
                .filter_map(|value| value.parse::<f32>().ok())
                .collect::<Vec<_>>();
            (values.len() == 6 && (values[0] - 20.0).abs() < 0.01).then_some(values[4])
        })
        .find(|x| *x > 500.0);

    assert!(transform_x.is_some());
}

#[test]
fn background_repeat_x_and_y_tile_only_requested_axis() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let render = |repeat: &str| {
        let raw = html2pdf_rs::render_html(&format!(
            "<style>div {{ background-image: url(data:image/png;base64,{encoded}); \
             background-size: 20pt 20pt; background-repeat: {repeat}; \
             line-height: 60pt }}</style><div>Latar</div>"
        ));
        String::from_utf8_lossy(&raw).into_owned()
    };

    let none = render("no-repeat");
    assert_eq!(image_draw_count(&none), 1);

    let repeat_x = render("repeat-x");
    let x_tiles = background_image_transforms(&repeat_x);
    assert!(x_tiles.len() > 1);
    assert!(x_tiles.iter().all(|values| (values[3] - 20.0).abs() < 0.01));

    let repeat_y = render("repeat-y");
    let y_tiles = background_image_transforms(&repeat_y);
    assert!(y_tiles.len() > 1);
    let first_x = y_tiles[0][4];
    assert!(
        y_tiles
            .iter()
            .all(|values| (values[4] - first_x).abs() < 0.01)
    );
}

#[test]
fn background_position_allows_negative_lengths() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let render = |position: &str| {
        let raw = html2pdf_rs::render_html(&format!(
            "<style>div {{ background-image: url(data:image/png;base64,{encoded}); \
             background-size: 20pt 20pt; background-repeat: no-repeat; \
             background-position: {position} }}</style><div>Latar</div>"
        ));
        background_image_transforms(&String::from_utf8_lossy(&raw))
            .into_iter()
            .map(|values| values[4])
            .fold(f32::INFINITY, f32::min)
    };

    let negative_x = render("-10pt 0");
    let positive_x = render("10pt 0");
    assert!((positive_x - negative_x - 20.0).abs() < 0.5);
}

#[test]
fn background_size_one_value_preserves_aspect_ratio() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [
        255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 255, 255, 0,
    ];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 4, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let render = |size: &str| {
        let raw = html2pdf_rs::render_html(&format!(
            "<style>div {{ background-image: url(data:image/png;base64,{encoded}); \
             background-repeat: no-repeat; background-size: {size} }}</style><div>Latar</div>"
        ));
        background_image_transforms(&String::from_utf8_lossy(&raw))[0]
    };

    let width_only = render("20pt");
    assert!((width_only[0] - 20.0).abs() < 0.01);
    assert!((width_only[3] - 10.0).abs() < 0.01);

    let height_only = render("auto 10pt");
    assert!((height_only[0] - 20.0).abs() < 0.01);
    assert!((height_only[3] - 10.0).abs() < 0.01);
}

#[test]
fn renders_hierarchical_bookmarks_from_headings() {
    let pdf = html2pdf_rs::render_html(
        "<h1>Bab Satu</h1><p>Isi</p><h2>Bagian A</h2><p>A</p>\
         <h1>Bab Dua</h1><p>B</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/Type /Outlines"));
    assert!(raw.contains("/PageMode /UseOutlines"));
    assert!(raw.contains("/Count 3"));
    let object_id = |title: &str| {
        let marker = format!("/Title ({title})");
        let lines = raw.lines().collect::<Vec<_>>();
        let position = lines.iter().position(|line| line.contains(&marker))?;
        lines[..position]
            .iter()
            .rev()
            .find(|line| line.contains(" obj"))?
            .split_whitespace()
            .next()
            .map(str::to_string)
    };
    let first = object_id("Bab Satu").expect("first heading object");
    let nested = object_id("Bagian A").expect("nested heading object");

    assert!(raw.contains(&format!("/Parent {first} 0 R")));
    assert!(nested != first);
}

#[test]
fn omits_outlines_without_headings() {
    let pdf = html2pdf_rs::render_html("<p>Tanpa judul</p>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(!raw.contains("/Type /Outlines"));
    assert!(!raw.contains("/PageMode /UseOutlines"));
}

#[test]
fn supports_counter_set() {
    let pdf = html2pdf_rs::render_html(
        "<style>counter { counter-reset: x 0 }\
         p { counter-set: x 5; counter-increment: x }\
         p::before { content: counter(x) }</style>\
         <counter><p>A</p></counter>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(6)"));
}

#[test]
fn supports_multiple_counters_per_declaration() {
    let pdf = html2pdf_rs::render_html(
        "<style>body { counter-reset: a 1 b 7 }\
         p { counter-increment: a b 2 }\
         p::before { content: counter(a) '-' counter(b) }</style>\
         <p>A</p>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(2-9)"), "missing multiple counter values");
}

#[test]
fn supports_custom_counter_style() {
    let pdf = html2pdf_rs::render_html(
        "<style>@counter-style ticks { system: cyclic; symbols: '*' '+'; suffix: '' }\
         ol { list-style-type: ticks }\
         li::before { content: counter(list-item, ticks) }</style>\
         <ol><li>A</li><li>B</li></ol>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("(*)"));
    assert!(raw.contains("(+)"));
}

fn encode_woff1(sfnt: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    let get_u16 = |offset: usize| u16::from_be_bytes(sfnt[offset..offset + 2].try_into().unwrap());
    let get_u32 = |offset: usize| u32::from_be_bytes(sfnt[offset..offset + 4].try_into().unwrap());
    let flavor = get_u32(0);
    let num_tables = get_u16(4) as usize;
    let mut directory = Vec::new();
    let mut payload = Vec::new();

    for index in 0..num_tables {
        let base = 12 + index * 16;
        let tag = get_u32(base);
        let checksum = get_u32(base + 4);
        let offset = get_u32(base + 8) as usize;
        let length = get_u32(base + 12) as usize;
        let original = &sfnt[offset..offset + length];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let encoded = encoder.finish().unwrap();
        let compressed = if encoded.len() < original.len() {
            encoded
        } else {
            original.to_vec()
        };
        let table_offset = 44 + num_tables * 20 + payload.len();
        directory.extend_from_slice(&tag.to_be_bytes());
        directory.extend_from_slice(&(table_offset as u32).to_be_bytes());
        directory.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        directory.extend_from_slice(&(length as u32).to_be_bytes());
        directory.extend_from_slice(&checksum.to_be_bytes());
        payload.extend_from_slice(&compressed);
        let padding = (4 - payload.len() % 4) % 4;
        payload.resize(payload.len() + padding, 0);
    }

    let mut woff = Vec::new();
    woff.extend_from_slice(b"wOFF");
    woff.extend_from_slice(&flavor.to_be_bytes());
    woff.extend_from_slice(&((44 + directory.len() + payload.len()) as u32).to_be_bytes());
    woff.extend_from_slice(&(num_tables as u16).to_be_bytes());
    woff.extend_from_slice(&0u16.to_be_bytes());
    woff.extend_from_slice(&(sfnt.len() as u32).to_be_bytes());
    woff.extend_from_slice(&1u16.to_be_bytes());
    woff.extend_from_slice(&0u16.to_be_bytes());
    woff.extend_from_slice(&0u32.to_be_bytes());
    woff.extend_from_slice(&0u32.to_be_bytes());
    woff.extend_from_slice(&0u32.to_be_bytes());
    woff.extend_from_slice(&0u32.to_be_bytes());
    woff.extend_from_slice(&0u32.to_be_bytes());
    woff.extend_from_slice(&directory);
    woff.extend_from_slice(&payload);
    woff
}

#[test]
fn writes_tagged_pdf_structure() {
    let pdf = html2pdf_rs::render_html("<html lang='id'><h1>Judul</h1><p>Isi</p></html>");
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/StructTreeRoot"));
    assert!(raw.contains("/MarkInfo"));
    assert!(raw.contains("/Marked true"));
    assert!(raw.contains("/Lang (id)"));
    assert!(raw.contains("/S /H1"));
    assert!(raw.contains("/ParentTree"));
    assert!(raw.contains("/MCID"));
}

#[test]
fn embeds_woff_font() {
    use std::fs;

    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/fonts");
    let woff = encode_woff1(&fs::read(source.join("DejaVuSans.ttf")).unwrap());
    let directory = std::env::temp_dir().join(format!("html2pdf-rs-woff-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("DejaVu.woff"), woff).unwrap();

    let pdf = html2pdf_rs::render_html_with_base(
        "<style>@font-face{font-family:W;src:url('DejaVu.woff')}\
         p{font-family:W}</style><p>Halo WOFF</p>",
        &directory,
    );
    fs::remove_dir_all(&directory).ok();
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/FontFile2"));
    assert!(raw.contains("Tj"));
}

#[test]
fn renders_acroform_fields() {
    let pdf = html2pdf_rs::render_html(
        "<form><input name='q' value='Halo'><input type='checkbox' name='ok' checked>\
         <select name='kota'><option>Jakarta</option><option selected>Bandung</option></select>\
         </form>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/AcroForm"));
    assert!(raw.contains("/FT /Tx"));
    assert!(raw.contains("/FT /Btn"));
    assert!(raw.contains("/FT /Ch"));
    assert!(raw.contains("/T (q)"));
    assert!(raw.contains("/T (ok)"));
    assert!(raw.contains("/V (Halo)"));
    assert!(raw.contains("/Opt [(Jakarta) (Bandung)]"));
    assert!(raw.contains("/AP <<"));
    assert!(raw.contains("/Subtype /Form"));
}

#[test]
fn embeds_document_and_inline_javascript() {
    let pdf = html2pdf_rs::render_html(
        "<script>app.alert('halo');</script><button onclick='app.alert(\"klik\")'>Klik</button>",
    );
    let raw = String::from_utf8_lossy(&pdf);

    assert!(raw.contains("/JavaScript <<"));
    assert!(raw.contains("/S /JavaScript"));
    assert!(raw.contains("app.alert('halo')"));
    assert!(raw.contains("app.alert(\"klik\")"));
}

#[test]
fn resolves_background_position_length_against_container_height() {
    use base64::Engine;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let mut png = Vec::new();
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 2, ExtendedColorType::Rgb8)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    let render_at = |offset: &str| {
        let pdf = html2pdf_rs::render_html(&format!(
            "<style>div {{ background-image: url(data:image/png;base64,{encoded}); \
             background-repeat: no-repeat; background-size: 20pt 20pt; line-height: 10; \
             background-position: right {offset} }}</style><div>X</div>"
        ));
        let raw = String::from_utf8_lossy(&pdf);
        raw.lines()
            .filter(|line| line.trim_end().ends_with(" cm"))
            .filter_map(|line| {
                let values = line
                    .trim()
                    .trim_end_matches(" cm")
                    .split_whitespace()
                    .filter_map(|value| value.parse::<f32>().ok())
                    .collect::<Vec<_>>();
                (values.len() == 6 && (values[0] - 20.0).abs() < 0.01).then_some(values[5])
            })
            .find(|y| *y > 0.0)
            .unwrap_or(0.0)
    };

    let top = render_at("0pt");
    let shifted = render_at("30pt");

    assert!((top - shifted - 30.0).abs() < 1.0);
}
