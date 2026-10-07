//! Test-only helpers: a dependency-free PNG writer/reader for golden images.
//!
//! PNG is used so a human (or an image viewer) can look at the golden. The
//! encoder writes uncompressed ("stored") deflate blocks, which keeps this
//! under 100 lines instead of pulling in a PNG crate for tests.

#![allow(dead_code, reason = "each test binary uses a subset")]

use tiny_skia::Pixmap;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).expect("small").to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Straight-alpha RGBA PNG of a premultiplied pixmap composited over `bg`
/// (so transparent areas are visible in a viewer), 8 bits per channel.
pub fn encode(pm: &Pixmap, bg: [u8; 3]) -> Vec<u8> {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for row in pm.data().chunks_exact(w * 4) {
        raw.push(0);
        for px in row.as_chunks::<4>().0 {
            for c in 0..3 {
                let over = u16::from(px[c]) + u16::from(bg[c]) * (255 - u16::from(px[3])) / 255;
                raw.push(u8::try_from(over.min(255)).expect("clamped"));
            }
        }
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = u16::try_from(block.len()).expect("block fits");
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&pm.width().to_be_bytes());
    ihdr.extend_from_slice(&pm.height().to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, *b"IHDR", &ihdr);
    chunk(&mut out, *b"IDAT", &z);
    chunk(&mut out, *b"IEND", &[]);
    out
}

/// Reads RGB triples back from a PNG written by [`encode`].
pub fn decode_rgb(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().expect("4 bytes"));
    let (w, h) = (be(16), be(20));
    let mut pos = 8;
    let mut z = Vec::new();
    while pos + 8 <= png.len() {
        let len = be(pos) as usize;
        if &png[pos + 4..pos + 8] == b"IDAT" {
            z.extend_from_slice(&png[pos + 8..pos + 8 + len]);
        }
        pos += 12 + len;
    }
    let mut raw = Vec::new();
    let mut at = 2;
    loop {
        let last = z[at] & 1 == 1;
        let len = usize::from(u16::from_le_bytes([z[at + 1], z[at + 2]]));
        raw.extend_from_slice(&z[at + 5..at + 5 + len]);
        at += 5 + len;
        if last {
            break;
        }
    }
    let rgb = raw
        .chunks_exact(w as usize * 3 + 1)
        .flat_map(|row| row[1..].iter().copied())
        .collect();
    (w, h, rgb)
}

/// Writes `pm` to `$MARKULI_DUMP_DIR/<name>.png` when that variable is set,
/// for looking at what the core rendered.
pub fn dump(name: &str, pm: &Pixmap, bg: [u8; 3]) {
    if let Ok(dir) = std::env::var("MARKULI_DUMP_DIR") {
        std::fs::write(format!("{dir}/{name}.png"), encode(pm, bg)).expect("write dump");
    }
}
