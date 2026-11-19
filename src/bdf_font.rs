//! Loading BDF bitmap fonts and drawing text with them, compatible with the fonts and text positioning of
//! [rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix).

use std::{
    collections::HashMap,
    fs,
    io::{self, BufRead, BufReader},
    path::Path,
};

use crate::Canvas;

/// Drawn in place of characters that the font doesn't have, if the font has it.
const REPLACEMENT_CODEPOINT: u32 = 0xFFFD;

/// A single character of a [`BdfFont`].
#[derive(Clone, Debug)]
pub struct Glyph {
    /// Character code point
    pub codepoint: u32,
    /// Glyph width in pixels
    pub width: u32,
    /// Glyph height in pixels
    pub height: u32,
    /// X offset (left bearing)
    pub x_offset: i32,
    /// Offset of the bottom row from the baseline, positive upwards
    pub y_offset: i32,
    /// Device width (advance width)
    pub device_width: u32,
    /// Bitmap data (packed bits, row by row, each row padded to whole bytes)
    pub bitmap: Vec<u8>,
}

/// A bitmap font in the BDF format.
#[derive(Clone, Debug)]
pub struct BdfFont {
    /// Font name
    pub name: String,
    /// Font size in points
    pub size: u32,
    /// Font height (ascent + descent)
    pub height: u32,
    /// Ascent (pixels above baseline)
    pub ascent: i32,
    /// Descent (pixels below baseline)
    pub descent: i32,
    /// Glyphs indexed by codepoint
    glyphs: HashMap<u32, Glyph>,
}

/// The whitespace separated value at `index` of a BDF line, e.g. `BBX 5 7 0 -1`.
fn field<T: std::str::FromStr>(line: &str, index: usize) -> Option<T> {
    line.split_whitespace().nth(index)?.parse().ok()
}

impl BdfFont {
    /// Load a BDF font from a file.
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::parse(BufReader::new(fs::File::open(path)?))
    }

    /// Load a BDF font from a file; same as [`BdfFont::load`].
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::load(path)
    }

    /// Parse a BDF font. Malformed glyphs are skipped.
    pub fn parse<R: BufRead>(mut reader: R) -> io::Result<Self> {
        let mut font = BdfFont {
            name: String::new(),
            size: 0,
            height: 0,
            ascent: 0,
            descent: 0,
            glyphs: HashMap::new(),
        };

        // The glyph being read, and whether we are inside its BITMAP section.
        let mut glyph: Option<Glyph> = None;
        let mut in_bitmap = false;

        let mut raw_line = Vec::new();
        loop {
            raw_line.clear();
            if reader.read_until(b'\n', &mut raw_line)? == 0 {
                break;
            }
            // Fonts often have Latin-1 copyright notices, which must not stop the parsing.
            let line = String::from_utf8_lossy(&raw_line);
            let line = line.trim();
            let keyword = line.split_whitespace().next().unwrap_or("");

            if in_bitmap {
                if keyword == "ENDCHAR" {
                    in_bitmap = false;
                    if let Some(glyph) = glyph.take() {
                        let bytes_per_row = glyph.width.div_ceil(8) as usize;
                        if glyph.bitmap.len() == bytes_per_row * glyph.height as usize {
                            font.glyphs.insert(glyph.codepoint, glyph);
                        }
                    }
                } else if let Some(glyph) = &mut glyph {
                    let bytes_per_row = glyph.width.div_ceil(8) as usize;
                    // Rows may be padded beyond the width, so only keep the bytes we need.
                    match hex_decode(line) {
                        Some(bytes) if bytes.len() >= bytes_per_row => {
                            glyph.bitmap.extend_from_slice(&bytes[..bytes_per_row]);
                        }
                        _ => return Err(invalid_data(format!("Invalid BDF bitmap row '{line}'"))),
                    }
                }
                continue;
            }

            match keyword {
                "FONT" => font.name = line["FONT".len()..].trim().to_string(),
                "SIZE" => font.size = field(line, 1).unwrap_or(0),
                "FONTBOUNDINGBOX" => font.height = field(line, 2).unwrap_or(0),
                "FONT_ASCENT" => font.ascent = field(line, 1).unwrap_or(0),
                "FONT_DESCENT" => font.descent = field(line, 1).unwrap_or(0),
                "STARTCHAR" => {
                    glyph = Some(Glyph {
                        codepoint: 0,
                        width: 0,
                        height: 0,
                        x_offset: 0,
                        y_offset: 0,
                        device_width: 0,
                        bitmap: Vec::new(),
                    });
                }
                "ENCODING" => {
                    // Glyphs without a Unicode code point have an encoding of -1, so they are dropped.
                    match (field::<u32>(line, 1), &mut glyph) {
                        (Some(codepoint), Some(glyph)) => glyph.codepoint = codepoint,
                        _ => glyph = None,
                    }
                }
                "DWIDTH" => {
                    if let Some(glyph) = &mut glyph {
                        glyph.device_width = field(line, 1).unwrap_or(0);
                    }
                }
                "BBX" => {
                    if let Some(glyph) = &mut glyph {
                        glyph.width = field(line, 1).unwrap_or(0);
                        glyph.height = field(line, 2).unwrap_or(0);
                        glyph.x_offset = field(line, 3).unwrap_or(0);
                        glyph.y_offset = field(line, 4).unwrap_or(0);
                    }
                }
                "BITMAP" => in_bitmap = true,
                "ENDCHAR" => glyph = None,
                _ => {}
            }
        }

        if font.glyphs.is_empty() {
            return Err(invalid_data("The BDF font has no glyphs".to_string()));
        }
        Ok(font)
    }

    /// Get a glyph by codepoint
    pub fn get_glyph(&self, codepoint: u32) -> Option<&Glyph> {
        self.glyphs.get(&codepoint)
    }

    /// The glyph drawn for a character: the character itself, or else the replacement character.
    fn glyph_to_draw(&self, codepoint: u32) -> Option<&Glyph> {
        self.get_glyph(codepoint)
            .or_else(|| self.get_glyph(REPLACEMENT_CODEPOINT))
    }

    /// How far drawing a character advances the cursor. Characters that the font doesn't have advance by the
    /// width of the replacement character, or by half the font size for a space, or else not at all.
    pub fn char_width(&self, codepoint: u32) -> u32 {
        match self.glyph_to_draw(codepoint) {
            Some(glyph) => glyph.device_width,
            None if codepoint == u32::from(' ') => self.size / 2,
            None => 0,
        }
    }

    /// The width that [`Canvas::draw_text`] advances by for this text, without extra spacing.
    pub fn text_width(&self, text: &str) -> u32 {
        text.chars().map(|c| self.char_width(c as u32)).sum()
    }

    /// Check if a glyph has bitmap data loaded (diagnostic)
    pub fn glyph_has_bitmap(&self, codepoint: u32) -> bool {
        self.get_glyph(codepoint)
            .is_some_and(|g| !g.bitmap.is_empty())
    }

    /// Get the number of bytes in a glyph's bitmap (diagnostic)
    pub fn glyph_bitmap_len(&self, codepoint: u32) -> usize {
        self.get_glyph(codepoint).map_or(0, |g| g.bitmap.len())
    }
}

fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl Canvas {
    /// Draw text with a BDF font, like `DrawText` of rpi-rgb-led-matrix.
    ///
    /// `y` is the baseline of the text, so to put the top of the text at `top`, pass `top + font.ascent`.
    /// `extra_spacing` is added after each character and can be negative.
    ///
    /// Returns how far the text advanced to the right, in pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        font: &BdfFont,
        x: i32,
        y: i32,
        r: u8,
        g: u8,
        b: u8,
        text: &str,
        extra_spacing: i32,
    ) -> i32 {
        let mut cursor_x = x;
        for ch in text.chars() {
            if let Some(glyph) = font.glyph_to_draw(ch as u32) {
                self.draw_glyph(glyph, cursor_x, y, r, g, b);
            }
            cursor_x += font.char_width(ch as u32) as i32 + extra_spacing;
        }
        cursor_x - x
    }

    /// Draw text from top to bottom with a BDF font, like `VerticalDrawText` of rpi-rgb-led-matrix.
    ///
    /// `y` is the baseline of the first character. Each character moves down by the font height plus
    /// `extra_spacing`. Returns how far the text advanced downwards, in pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_vertical(
        &mut self,
        font: &BdfFont,
        x: i32,
        y: i32,
        r: u8,
        g: u8,
        b: u8,
        text: &str,
        extra_spacing: i32,
    ) -> i32 {
        let mut cursor_y = y;
        for ch in text.chars() {
            if let Some(glyph) = font.glyph_to_draw(ch as u32) {
                self.draw_glyph(glyph, x, cursor_y, r, g, b);
            }
            cursor_y += font.height as i32 + extra_spacing;
        }
        cursor_y - y
    }

    /// Draw a glyph with its baseline at `y`.
    fn draw_glyph(&mut self, glyph: &Glyph, x: i32, y: i32, r: u8, g: u8, b: u8) {
        let left = x + glyph.x_offset;
        let top = y - glyph.height as i32 - glyph.y_offset;
        let bytes_per_row = glyph.width.div_ceil(8) as usize;

        for (row, row_bytes) in glyph.bitmap.chunks_exact(bytes_per_row).enumerate() {
            for col in 0..glyph.width as usize {
                if row_bytes[col / 8] & (0x80 >> (col % 8)) == 0 {
                    continue;
                }
                let (pixel_x, pixel_y) = (left + col as i32, top + row as i32);
                // Pixels beyond the right and bottom edges are ignored by `set_pixel`.
                if pixel_x >= 0 && pixel_y >= 0 {
                    self.set_pixel(pixel_x as usize, pixel_y as usize, r, g, b);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BdfFont;

    const FONT: &str = "STARTFONT 2.1
COMMENT Copyright \u{a9} somebody
FONT -Test-8
SIZE 8 75 75
FONTBOUNDINGBOX 5 8 0 -1
FONT_ASCENT 7
FONT_DESCENT 1
CHARS 3
STARTCHAR exclam
ENCODING 33
SWIDTH 400 0
DWIDTH 2 0
BBX 1 7 0 0
BITMAP
80
80
80
80
00
80
80
ENDCHAR
STARTCHAR g
ENCODING 103
DWIDTH 5 0
BBX 4 3 0 -1
BITMAP
F0
90
F0
ENDCHAR
STARTCHAR unmapped
ENCODING -1
DWIDTH 9 0
BBX 1 1 0 0
BITMAP
80
ENDCHAR
ENDFONT
";

    #[test]
    fn test_parse() {
        let font = BdfFont::parse(FONT.as_bytes()).unwrap();
        assert_eq!(font.name, "-Test-8");
        assert_eq!(
            (font.size, font.height, font.ascent, font.descent),
            (8, 8, 7, 1)
        );

        let exclam = font.get_glyph('!' as u32).unwrap();
        assert_eq!(
            (exclam.width, exclam.height, exclam.device_width),
            (1, 7, 2)
        );
        assert_eq!(exclam.bitmap, [0x80, 0x80, 0x80, 0x80, 0x00, 0x80, 0x80]);

        let g = font.get_glyph('g' as u32).unwrap();
        assert_eq!(g.y_offset, -1);
        assert!(
            font.get_glyph(0).is_none(),
            "the unmapped glyph must be dropped"
        );
    }

    #[test]
    fn test_parse_latin1() {
        let mut bytes = FONT.replace('\u{a9}', "").into_bytes();
        bytes.splice(30..30, [0xA9]);
        assert!(BdfFont::parse(bytes.as_slice()).is_ok());
    }

    #[test]
    fn test_load_bundled_font() {
        let font =
            BdfFont::load(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/test_font.bdf")).unwrap();
        assert!(font.get_glyph('!' as u32).is_some());
    }

    #[test]
    fn test_text_width() {
        let font = BdfFont::parse(FONT.as_bytes()).unwrap();
        assert_eq!(font.text_width("!g!"), 9);
        // No space glyph, so half the font size.
        assert_eq!(font.text_width(" "), 4);
        assert_eq!(font.text_width("x"), 0);
    }
}
