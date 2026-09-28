//! The small, safe subset of CSS a note's `style="…"` may use: colours,
//! font size, weight and style, underline and strike-through, and text
//! alignment. Everything else, including anything that moves or sizes
//! boxes, is ignored, so a note can't break the layout around it.

use std::fmt;

use super::kinds::Alignment;

/// An sRGB colour with alpha, as written in a note.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const fn rgb(value: u32) -> Self {
        Self {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
            a: 255,
        }
    }

    /// The colour as `0xRRGGBBAA`.
    pub fn to_u32(self) -> u32 {
        u32::from_be_bytes([self.r, self.g, self.b, self.a])
    }

    pub fn from_u32(value: u32) -> Self {
        let [r, g, b, a] = value.to_be_bytes();
        Self { r, g, b, a }
    }

    /// `#rrggbb`, or `#rrggbbaa` when it isn't opaque.
    pub fn to_hex(self) -> String {
        match self.a {
            255 => format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b),
            a => format!("#{:02x}{:02x}{:02x}{a:02x}", self.r, self.g, self.b),
        }
    }
}

impl fmt::Debug for Rgba8 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// The smallest and largest text a note may ask for, as percentages of
/// the text around it.
pub const FONT_SCALE_RANGE: (u16, u16) = (50, 300);

/// A `font-size`, as a percentage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSize {
    /// Of the note's body text: `px`, `rem` and keywords such as `large`.
    Absolute(u16),
    /// Of the surrounding text: `em` and `%`.
    Relative(u16),
}

/// The styles one element's `style` attribute asks for.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct HtmlStyle {
    pub color: Option<Rgba8>,
    pub background: Option<Rgba8>,
    pub font_size: Option<FontSize>,
    /// `Some(true)` for bold, `Some(false)` for normal weight.
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: bool,
    pub strikethrough: bool,
    /// `text-align`, which only block elements honour.
    pub align: Option<Alignment>,
}

impl fmt::Debug for HtmlStyle {
    /// The declarations the style keeps, as CSS, so tree dumps stay short.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{{{}}}", self.to_css())
    }
}

impl HtmlStyle {
    /// No style at all.
    pub const NONE: Self = Self {
        color: None,
        background: None,
        font_size: None,
        bold: None,
        italic: None,
        underline: false,
        strikethrough: false,
        align: None,
    };

    /// Reads a `style` attribute, keeping only the supported properties.
    pub fn parse(style: &str) -> Self {
        let mut parsed = Self::default();
        for declaration in style.split(';') {
            if let Some((property, value)) = declaration.split_once(':') {
                parsed.apply(property.trim(), value.trim());
            }
        }
        parsed
    }

    fn apply(&mut self, property: &str, value: &str) {
        let value = value
            .trim_end_matches("!important")
            .trim()
            .to_ascii_lowercase();
        match property.to_ascii_lowercase().as_str() {
            "color" => self.color = parse_color(&value).or(self.color),
            "background-color" | "background" => {
                self.background = parse_color(&value).or(self.background)
            }
            "font-size" => self.font_size = parse_font_size(&value).or(self.font_size),
            "font-weight" => self.bold = parse_weight(&value).or(self.bold),
            "font-style" => self.italic = parse_italic(&value).or(self.italic),
            "text-decoration" | "text-decoration-line" => self.apply_decoration(&value),
            "text-align" => self.align = parse_align(&value).or(self.align),
            _ => {}
        }
    }

    fn apply_decoration(&mut self, value: &str) {
        for word in value.split_whitespace() {
            match word {
                "underline" => self.underline = true,
                "line-through" => self.strikethrough = true,
                "none" => (self.underline, self.strikethrough) = (false, false),
                _ => {}
            }
        }
    }

    /// Whether the style asks for nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The style as CSS built only from what was understood, so it is safe
    /// to write into an exported page.
    pub fn to_css(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(color) = self.color {
            parts.push(format!("color: {}", color.to_hex()));
        }
        if let Some(color) = self.background {
            parts.push(format!("background-color: {}", color.to_hex()));
        }
        if let Some(size) = self.font_size {
            parts.push(format!("font-size: {}", font_size_css(size)));
        }
        if let Some(bold) = self.bold {
            parts.push(format!(
                "font-weight: {}",
                if bold { "bold" } else { "normal" }
            ));
        }
        if let Some(italic) = self.italic {
            parts.push(format!(
                "font-style: {}",
                if italic { "italic" } else { "normal" }
            ));
        }
        if let Some(decoration) = self.decoration_css() {
            parts.push(format!("text-decoration: {decoration}"));
        }
        if let Some(align) = self.align.and_then(align_css) {
            parts.push(format!("text-align: {align}"));
        }
        parts.join("; ")
    }

    fn decoration_css(&self) -> Option<&'static str> {
        match (self.underline, self.strikethrough) {
            (true, true) => Some("underline line-through"),
            (true, false) => Some("underline"),
            (false, true) => Some("line-through"),
            (false, false) => None,
        }
    }
}

fn font_size_css(size: FontSize) -> String {
    match size {
        FontSize::Absolute(percent) => format!("{}rem", f32::from(percent) / 100.),
        FontSize::Relative(percent) => format!("{percent}%"),
    }
}

fn align_css(align: Alignment) -> Option<&'static str> {
    match align {
        Alignment::Left => Some("left"),
        Alignment::Center => Some("center"),
        Alignment::Right => Some("right"),
        Alignment::None => None,
    }
}

/// A `text-align` value, or an HTML `align` attribute.
pub fn parse_align(value: &str) -> Option<Alignment> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => Some(Alignment::Left),
        "center" => Some(Alignment::Center),
        "right" | "end" => Some(Alignment::Right),
        _ => None,
    }
}

fn parse_weight(value: &str) -> Option<bool> {
    match value {
        "bold" | "bolder" => Some(true),
        "normal" | "lighter" => Some(false),
        number => number.parse::<u16>().ok().map(|weight| weight >= 600),
    }
}

fn parse_italic(value: &str) -> Option<bool> {
    match value {
        "normal" => Some(false),
        _ if value.starts_with("italic") || value.starts_with("oblique") => Some(true),
        _ => None,
    }
}

const SIZE_KEYWORDS: [(&str, u16); 7] = [
    ("xx-small", 60),
    ("x-small", 75),
    ("small", 89),
    ("medium", 100),
    ("large", 120),
    ("x-large", 150),
    ("xx-large", 200),
];

/// Pixels in the body text CSS assumes, for reading `px` sizes.
const CSS_BODY_PX: f32 = 16.;

fn parse_font_size(value: &str) -> Option<FontSize> {
    if let Some((_, percent)) = SIZE_KEYWORDS.iter().find(|(name, _)| *name == value) {
        return Some(FontSize::Absolute(*percent));
    }
    let (number, relative) = [
        ("px", false),
        ("rem", false),
        ("em", true),
        ("%", true),
        ("pt", false),
    ]
    .iter()
    .find_map(|(unit, relative)| Some((size_in(value, unit)?, *relative)))?;
    let clamped = number.clamp(f32::from(FONT_SCALE_RANGE.0), f32::from(FONT_SCALE_RANGE.1)) as u16;
    Some(match relative {
        true => FontSize::Relative(clamped),
        false => FontSize::Absolute(clamped),
    })
}

/// A length in `unit`, as a percentage of the body size.
fn size_in(value: &str, unit: &str) -> Option<f32> {
    let number: f32 = value.strip_suffix(unit)?.trim().parse().ok()?;
    let percent = match unit {
        "px" => number / CSS_BODY_PX * 100.,
        "pt" => number * 4. / 3. / CSS_BODY_PX * 100.,
        "%" => number,
        _ => number * 100.,
    };
    (percent.is_finite() && percent > 0.).then_some(percent)
}

/// A CSS colour: a name, `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`,
/// `rgba()`, `hsl()` or `hsla()`.
pub fn parse_color(value: &str) -> Option<Rgba8> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some((function, arguments)) = value.strip_suffix(')').and_then(|v| v.split_once('(')) {
        return parse_function(function.trim(), arguments);
    }
    named_color(&value)
}

fn parse_hex(hex: &str) -> Option<Rgba8> {
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |at: usize| u8::from_str_radix(&hex[at..at + 1], 16).unwrap_or(0);
    let pair = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    let color = match hex.len() {
        3 | 4 => Rgba8 {
            r: digit(0) * 17,
            g: digit(1) * 17,
            b: digit(2) * 17,
            a: if hex.len() == 4 { digit(3) * 17 } else { 255 },
        },
        6 | 8 => Rgba8 {
            r: pair(0),
            g: pair(2),
            b: pair(4),
            a: if hex.len() == 8 { pair(6) } else { 255 },
        },
        _ => return None,
    };
    Some(color)
}

fn parse_function(function: &str, arguments: &str) -> Option<Rgba8> {
    let parts: Vec<&str> = arguments
        .split([',', ' ', '/'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let alpha = match parts.get(3) {
        Some(alpha) => fraction(alpha)?,
        None if parts.len() == 3 => 1.,
        None => return None,
    };
    let [first, second, third] = [parts[0], parts[1], parts[2]];
    let (r, g, b) = match function {
        "rgb" | "rgba" => (channel(first)?, channel(second)?, channel(third)?),
        "hsl" | "hsla" => hsl_to_rgb(hue(first)?, fraction(second)?, fraction(third)?),
        _ => return None,
    };
    Some(Rgba8 {
        r,
        g,
        b,
        a: unit_to_byte(alpha),
    })
}

fn unit_to_byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

/// An `rgb()` channel: 0 to 255, or a percentage.
fn channel(part: &str) -> Option<u8> {
    match part.strip_suffix('%') {
        Some(percent) => Some(unit_to_byte(percent.parse::<f32>().ok()? / 100.)),
        None => Some(part.parse::<f32>().ok()?.clamp(0., 255.).round() as u8),
    }
}

/// A number from 0 to 1, or a percentage.
fn fraction(part: &str) -> Option<f32> {
    match part.strip_suffix('%') {
        Some(percent) => Some(percent.parse::<f32>().ok()? / 100.),
        None => part.parse::<f32>().ok(),
    }
    .filter(|value| value.is_finite())
    .map(|value| value.clamp(0., 1.))
}

fn hue(part: &str) -> Option<f32> {
    let degrees: f32 = part.trim_end_matches("deg").parse().ok()?;
    degrees.is_finite().then(|| degrees.rem_euclid(360.))
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (u8, u8, u8) {
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let sector = hue / 60.;
    let second = chroma * (1. - (sector % 2. - 1.).abs());
    let (r, g, b) = match sector as u8 {
        0 => (chroma, second, 0.),
        1 => (second, chroma, 0.),
        2 => (0., chroma, second),
        3 => (0., second, chroma),
        4 => (second, 0., chroma),
        _ => (chroma, 0., second),
    };
    let base = lightness - chroma / 2.;
    (
        unit_to_byte(r + base),
        unit_to_byte(g + base),
        unit_to_byte(b + base),
    )
}

fn named_color(name: &str) -> Option<Rgba8> {
    if name == "transparent" {
        return Some(Rgba8::from_u32(0));
    }
    NAMED_COLORS
        .binary_search_by(|(candidate, _)| candidate.cmp(&name))
        .ok()
        .map(|index| Rgba8::rgb(NAMED_COLORS[index].1))
}

/// The CSS named colours, sorted by name.
const NAMED_COLORS: [(&str, u32); 148] = [
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_colours_are_sorted() {
        assert!(NAMED_COLORS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn colours_in_every_notation() {
        let red = Some(Rgba8::rgb(0xff0000));
        for written in [
            "red",
            "RED",
            "#f00",
            "#ff0000",
            "rgb(255, 0, 0)",
            "rgb(100% 0% 0%)",
            "hsl(0, 100%, 50%)",
            "hsl(360deg 100% 50%)",
        ] {
            assert_eq!(parse_color(written), red, "{written}");
        }
        assert_eq!(
            parse_color("rgba(0, 0, 255, 0.5)"),
            Some(Rgba8 {
                r: 0,
                g: 0,
                b: 255,
                a: 128
            })
        );
        assert_eq!(
            parse_color("#11223344").map(Rgba8::to_u32),
            Some(0x11223344)
        );
        assert_eq!(
            parse_color("hsl(120, 100%, 25%)"),
            Some(Rgba8::rgb(0x008000))
        );
        for bad in ["redd", "#12", "#ggg", "rgb(1,2)", "url(x)", "calc(1px)", ""] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn supported_properties_are_read_and_others_ignored() {
        let style = HtmlStyle::parse(
            "color:red; background-color: #ff0; font-size: 24px; font-weight: 700; \
             font-style: italic; text-decoration: underline line-through; text-align: center; \
             position: fixed; width: 9999px; display: none; margin: -100px",
        );
        assert_eq!(style.color, Some(Rgba8::rgb(0xff0000)));
        assert_eq!(style.background, Some(Rgba8::rgb(0xffff00)));
        assert_eq!(style.font_size, Some(FontSize::Absolute(150)));
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.italic, Some(true));
        assert!(style.underline && style.strikethrough);
        assert_eq!(style.align, Some(Alignment::Center));
        assert_eq!(
            style.to_css(),
            "color: #ff0000; background-color: #ffff00; font-size: 1.5rem; \
             font-weight: bold; font-style: italic; text-decoration: underline line-through; \
             text-align: center"
        );
        assert!(HtmlStyle::parse("position: absolute; top: 0").is_empty());
    }

    #[test]
    fn font_sizes_are_clamped() {
        let size = |css: &str| HtmlStyle::parse(css).font_size;
        assert_eq!(size("font-size: 200px"), Some(FontSize::Absolute(300)));
        assert_eq!(size("font-size: 1px"), Some(FontSize::Absolute(50)));
        assert_eq!(size("font-size: 2em"), Some(FontSize::Relative(200)));
        assert_eq!(size("font-size: 80%"), Some(FontSize::Relative(80)));
        assert_eq!(size("font-size: 1.25rem"), Some(FontSize::Absolute(125)));
        assert_eq!(size("font-size: large"), Some(FontSize::Absolute(120)));
        assert_eq!(size("font-size: calc(100vh)"), None);
        assert_eq!(size("font-size: -3px"), None);
    }

    #[test]
    fn a_style_debugs_as_its_css() {
        let style = HtmlStyle::parse("color: red");
        assert_eq!(format!("{style:?}"), "{color: #ff0000}");
    }
}
