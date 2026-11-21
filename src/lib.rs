use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::css::paper_size;

pub mod css;
pub mod layout;
pub mod pdf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageTextLocation {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl PageTextLocation {
    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_'], "")
            .as_str()
        {
            "topleft" => Some(Self::TopLeft),
            "topcenter" => Some(Self::TopCenter),
            "topright" => Some(Self::TopRight),
            "bottomleft" => Some(Self::BottomLeft),
            "bottomcenter" => Some(Self::BottomCenter),
            "bottomright" => Some(Self::BottomRight),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourcePolicy {
    pub remote_enabled: bool,
    pub allowed_remote_hosts: Vec<String>,
    pub chroot: Option<PathBuf>,
}

impl ResourcePolicy {
    pub fn remote_enabled(mut self) -> Self {
        self.remote_enabled = true;
        self
    }

    pub fn allow_remote_host(mut self, host: &str) -> Self {
        let host = host.trim().to_ascii_lowercase();
        if !host.is_empty() && !self.allowed_remote_hosts.contains(&host) {
            self.allowed_remote_hosts.push(host);
        }
        self
    }

    pub fn chroot(mut self, path: &str) -> Self {
        if !path.trim().is_empty() {
            self.chroot = Some(PathBuf::from(path));
        }
        self
    }

    fn allows_remote_url(&self, src: &str) -> bool {
        if !self.remote_enabled {
            return false;
        }
        if self.allowed_remote_hosts.is_empty() {
            return true;
        }

        let Some(host) = remote_host(src) else {
            return false;
        };
        self.allowed_remote_hosts
            .iter()
            .any(|allowed| host == allowed.as_str() || host.ends_with(&format!(".{allowed}")))
    }
}

fn is_remote(src: &str) -> bool {
    src.starts_with("http://") || src.starts_with("https://")
}

fn remote_host(src: &str) -> Option<String> {
    let rest = src.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

pub fn read_bytes(src: &str, base: &Path, policy: &ResourcePolicy) -> Option<Vec<u8>> {
    if let Some(position) = src.find(";base64,") {
        return base64::engine::general_purpose::STANDARD
            .decode(&src[position + ";base64,".len()..])
            .ok();
    }

    if is_remote(src) {
        if !policy.allows_remote_url(src) {
            return None;
        }
        let response = ureq::get(src).call().ok()?;
        let mut bytes = Vec::new();
        response.into_reader().read_to_end(&mut bytes).ok()?;
        return Some(bytes);
    }

    let path = base.join(src);
    if let Some(root) = &policy.chroot {
        let root = std::fs::canonicalize(root).ok()?;
        let path = std::fs::canonicalize(&path).ok()?;
        if !path.starts_with(root) {
            return None;
        }
    }
    std::fs::read(path).ok()
}

pub fn read_text(src: &str, base: &Path, policy: &ResourcePolicy) -> Option<String> {
    String::from_utf8(read_bytes(src, base, policy)?).ok()
}

pub fn resource_base(src: &str, base: &Path) -> PathBuf {
    if src.starts_with("data:") || is_remote(src) {
        return base.to_path_buf();
    }
    base.join(src)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.to_path_buf())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub width: f32,
    pub height: f32,
    pub margin: f32,
    pub default_font: String,
    pub dpi: f32,
    pub image_byte_limit: Option<usize>,
    pub resource: ResourcePolicy,
    pub page_text: Option<String>,
    pub page_text_location: PageTextLocation,
    pub user_password: Option<String>,
    pub owner_password: Option<String>,
    pub pdf_a: bool,
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
            resource: ResourcePolicy::default(),
            page_text: None,
            page_text_location: PageTextLocation::BottomCenter,
            user_password: None,
            owner_password: None,
            pdf_a: false,
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

    pub fn remote_resources(mut self) -> Self {
        self.resource = self.resource.remote_enabled();
        self
    }

    pub fn allow_remote_host(mut self, host: &str) -> Self {
        self.resource = self.resource.allow_remote_host(host);
        self
    }

    pub fn chroot(mut self, path: &str) -> Self {
        self.resource = self.resource.chroot(path);
        self
    }

    pub fn page_text(mut self, text: &str) -> Self {
        self.page_text = Some(text.to_string());
        self
    }

    pub fn page_text_location(mut self, location: PageTextLocation) -> Self {
        self.page_text_location = location;
        self
    }

    pub fn encrypt(mut self, user_password: &str) -> Self {
        self.user_password = Some(user_password.to_string());
        self
    }

    pub fn owner_password(mut self, owner_password: &str) -> Self {
        self.owner_password = Some(owner_password.to_string());
        self
    }

    pub fn pdf_a(mut self) -> Self {
        self.pdf_a = true;
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
