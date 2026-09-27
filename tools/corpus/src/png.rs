//! A minimal PNG encoder for the tiny attachments (uncompressed deflate blocks).

/// Encodes an RGB image. `pixels` holds `width * height` colours, row by row.
pub fn encode_rgb(width: u32, height: u32, pixels: &[[u8; 3]]) -> Vec<u8> {
    assert_eq!(
        pixels.len(),
        (width * height) as usize,
        "pixel count must match size"
    );
    let mut raw = Vec::with_capacity(pixels.len() * 3 + height as usize);
    for row in pixels.chunks(width as usize) {
        raw.push(0);
        for pixel in row {
            raw.extend_from_slice(pixel);
        }
    }
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    push_chunk(&mut png, b"IHDR", &header);
    push_chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());
}

/// Wraps data in a zlib stream made of one stored (uncompressed) block.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    assert!(data.len() <= u16::MAX as usize, "tiny images only");
    let len = data.len() as u16;
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn adler_matches_known_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_has_signature_and_is_tiny() {
        let png = encode_rgb(2, 2, &[[255, 0, 0], [0, 255, 0], [0, 0, 255], [9, 9, 9]]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
        assert!(png.len() < 100);
    }
}
