use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let input = args.next().unwrap_or_else(|| "-".to_string());
    let output = args.next().unwrap_or_else(|| "out.pdf".to_string());

    let base = if input == "-" {
        PathBuf::from(".")
    } else {
        Path::new(&input)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    let html = if input == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        fs::read_to_string(&input)?
    };

    fs::write(output, html2pdf_rs::render_html_with_base(&html, &base))
}
