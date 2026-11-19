use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use html2pdf_rs::Options;

fn help() {
    println!("Usage: html2pdf-rs [options] [input.html|-] [output.pdf]");
    println!();
    println!("Options:");
    println!("  --paper NAME   a4, letter, legal, a3 (default: a4)");
    println!("  --landscape    Swap paper width and height");
    println!("  --margin PT    Page margin in points (default: 54)");
    println!("  --help         Show this help");
}

fn parse_args() -> io::Result<(Options, String, String)> {
    let mut options = Options::default();
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                help();
                std::process::exit(0);
            }
            "--landscape" => options = options.landscape(),
            "--paper" => {
                let value = args.next().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--paper needs a value")
                })?;
                options = Options::paper(&value).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown paper: {value}"),
                    )
                })?;
            }
            "--margin" => {
                let value = args.next().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--margin needs a value")
                })?;
                let margin = value.parse::<f32>().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("invalid margin: {value}"),
                    )
                })?;
                options = options.margin(margin);
            }
            _ if arg.starts_with('-') && arg != "-" => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {arg}"),
                ));
            }
            _ => positional.push(arg),
        }
    }

    let input = positional
        .first()
        .cloned()
        .unwrap_or_else(|| "-".to_string());
    let output = positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| "out.pdf".to_string());

    Ok((options, input, output))
}

fn main() -> io::Result<()> {
    let (options, input, output) = parse_args()?;

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

    let pdf = html2pdf_rs::render_html_with_options(&html, &base, &options);
    let mut file = fs::File::create(output)?;
    file.write_all(&pdf)
}
