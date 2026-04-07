/// RGB color with components in 0.0..=1.0.
#[derive(Debug, Clone, Copy)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Create from a hex color like 0x4A90D9.
    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as f32 / 255.0,
            g: ((hex >> 8) & 0xFF) as f32 / 255.0,
            b: (hex & 0xFF) as f32 / 255.0,
        }
    }
}

/// Map a file extension to a color.
///
/// Grouped by category:
/// - Images: blues
/// - Video: purples
/// - Audio: greens
/// - Documents: oranges
/// - Code: teals
/// - Archives: reds
/// - Default: slate gray
pub fn color_for_extension(ext: Option<&str>) -> Rgb {
    let ext = match ext {
        Some(e) => e.to_ascii_lowercase(),
        None => return DEFAULT_COLOR,
    };

    match ext.as_str() {
        // Images — blues
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "svg" | "ico" | "heic" => {
            Rgb::from_hex(0x4A90D9)
        }
        "raw" | "cr2" | "nef" | "arw" => Rgb::from_hex(0x5B9BD5),

        // Video — purples
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" => {
            Rgb::from_hex(0x8B5CF6)
        }

        // Audio — greens
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "wma" | "m4a" | "opus" => {
            Rgb::from_hex(0x22C55E)
        }

        // Documents — oranges
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => {
            Rgb::from_hex(0xF59E0B)
        }
        "txt" | "md" | "rtf" | "csv" | "tsv" => Rgb::from_hex(0xFBBF24),

        // Code — teals
        "rs" | "go" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" => {
            Rgb::from_hex(0x14B8A6)
        }
        "java" | "kt" | "swift" | "rb" | "php" | "cs" | "scala" | "zig" => {
            Rgb::from_hex(0x0D9488)
        }
        "html" | "css" | "scss" | "less" | "xml" | "json" | "yaml" | "yml" | "toml" => {
            Rgb::from_hex(0x0F766E)
        }

        // Archives — reds
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4" => {
            Rgb::from_hex(0xEF4444)
        }
        "deb" | "rpm" | "dmg" | "iso" | "img" => Rgb::from_hex(0xDC2626),

        // Executables / libraries — warm gray
        "exe" | "dll" | "so" | "dylib" | "app" | "bin" | "elf" => Rgb::from_hex(0x78716C),

        // Databases — indigo
        "db" | "sqlite" | "sqlite3" | "mdb" => Rgb::from_hex(0x6366F1),

        // Fonts — pink
        "ttf" | "otf" | "woff" | "woff2" => Rgb::from_hex(0xEC4899),

        _ => DEFAULT_COLOR,
    }
}

/// Color for directories — muted blue-gray.
pub const DIR_COLOR: Rgb = Rgb::from_hex(0x64748B);

/// Color for denied/restricted directories — dark red-gray.
pub const DENIED_COLOR: Rgb = Rgb::from_hex(0x7F1D1D);

const DEFAULT_COLOR: Rgb = Rgb::from_hex(0x94A3B8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_return_non_default() {
        let jpg = color_for_extension(Some("jpg"));
        let def = color_for_extension(Some("xyz_unknown_abc"));
        // jpg should be blue-ish, not the default slate
        assert!((jpg.r - def.r).abs() > 0.01 || (jpg.b - def.b).abs() > 0.01);
    }

    #[test]
    fn case_insensitive() {
        let lower = color_for_extension(Some("jpg"));
        let upper = color_for_extension(Some("JPG"));
        assert!((lower.r - upper.r).abs() < f32::EPSILON);
    }

    #[test]
    fn none_extension_returns_default() {
        let c = color_for_extension(None);
        assert!((c.r - DEFAULT_COLOR.r).abs() < f32::EPSILON);
    }
}
