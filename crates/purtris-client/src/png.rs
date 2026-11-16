//! Minimal PNG writer (stored deflate blocks). Used for headless screenshots.

fn crc32(data: &[u8], mut c: u32) -> u32 {
    c = !c;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

pub fn write(path: &str, w: usize, h: usize, px: &[u32]) -> std::io::Result<()> {
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let c = px[y * w + x];
            raw.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, ch) in chunks.iter().enumerate() {
        z.push(if i + 1 == chunks.len() { 1 } else { 0 });
        let n = ch.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(ch);
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let chunk = |typ: &[u8], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = typ.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c, 0).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr, &mut out);
    chunk(b"IDAT", &z, &mut out);
    chunk(b"IEND", &[], &mut out);
    std::fs::write(path, out)
}

// ------------------------------------------------------------------ decoding (written from scratch)

/// Decode an 8-bit, non-interlaced PNG (RGBA, RGB, gray or gray+alpha) into a pixel canvas.
pub fn decode(data: &[u8]) -> Option<crate::gfx::Canvas> {
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut p = 8;
    let (mut w, mut h, mut ctype, mut depth, mut interlace) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let mut idat = Vec::new();
    while p + 8 <= data.len() {
        let len = u32::from_be_bytes(data[p..p + 4].try_into().ok()?) as usize;
        let kind = &data[p + 4..p + 8];
        let body = data.get(p + 8..p + 8 + len)?;
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes(body[0..4].try_into().ok()?) as usize;
                h = u32::from_be_bytes(body[4..8].try_into().ok()?) as usize;
                depth = body[8];
                ctype = body[9];
                interlace = body[12];
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        p += 12 + len;
    }
    if depth != 8 || interlace != 0 || w == 0 || h == 0 {
        return None;
    }
    let bpp = match ctype {
        6 => 4,
        2 => 3,
        4 => 2,
        0 => 1,
        _ => return None,
    };
    let raw = inflate(idat.get(2..)?)?; // skip the zlib header
    let stride = w * bpp;
    if raw.len() < (stride + 1) * h {
        return None;
    }
    let mut cv = crate::gfx::Canvas::transparent(w as i32, h as i32);
    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    for y in 0..h {
        let row = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let f = row[0];
        for i in 0..stride {
            let x = row[1 + i];
            let a = if i >= bpp { cur[i - bpp] as i32 } else { 0 };
            let b = prev[i] as i32;
            let c = if i >= bpp { prev[i - bpp] as i32 } else { 0 };
            cur[i] = match f {
                0 => x,
                1 => x.wrapping_add(a as u8),
                2 => x.wrapping_add(b as u8),
                3 => x.wrapping_add(((a + b) / 2) as u8),
                4 => {
                    let pp = a + b - c;
                    let (pa, pb, pc) = ((pp - a).abs(), (pp - b).abs(), (pp - c).abs());
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                    x.wrapping_add(pr as u8)
                }
                _ => return None,
            };
        }
        for xx in 0..w {
            let s = &cur[xx * bpp..xx * bpp + bpp];
            let (r, g, b, a) = match bpp {
                4 => (s[0], s[1], s[2], s[3]),
                3 => (s[0], s[1], s[2], 255),
                2 => (s[0], s[0], s[0], s[1]),
                _ => (s[0], s[0], s[0], 255),
            };
            cv.px[y * w + xx] = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    Some(cv)
}

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}
impl<'a> Bits<'a> {
    fn need(&mut self, n: u32) -> Option<()> {
        while self.nbits < n {
            let byte = *self.d.get(self.pos)?;
            self.pos += 1;
            self.bit |= (byte as u32) << self.nbits;
            self.nbits += 8;
        }
        Some(())
    }
    fn get(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Some(v)
    }
    fn align(&mut self) {
        let drop = self.nbits % 8;
        self.bit >>= drop;
        self.nbits -= drop;
    }
}

/// Canonical Huffman table: counts per length and symbols in code order.
struct Huff {
    counts: [u16; 16],
    syms: Vec<u16>,
}
impl Huff {
    fn new(lens: &[u8]) -> Huff {
        let mut counts = [0u16; 16];
        for &l in lens {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut syms = vec![0u16; lens.len()];
        for (s, &l) in lens.iter().enumerate() {
            if l != 0 {
                syms[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { counts, syms }
    }
    fn decode(&self, b: &mut Bits) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return self.syms.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Raw DEFLATE (RFC 1951) decoder.
pub fn inflate(d: &[u8]) -> Option<Vec<u8>> {
    let mut b = Bits { d, pos: 0, bit: 0, nbits: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(d.len() * 4);
    loop {
        let last = b.get(1)?;
        let kind = b.get(2)?;
        match kind {
            0 => {
                b.align();
                let len = b.get(16)? as usize;
                let _nlen = b.get(16)?;
                for _ in 0..len {
                    out.push(b.get(8)? as u8);
                }
            }
            1 | 2 => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [0u8; 288];
                    for (i, v) in l.iter_mut().enumerate() {
                        *v = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
                    }
                    (Huff::new(&l), Huff::new(&[5u8; 30]))
                } else {
                    let hlit = b.get(5)? as usize + 257;
                    let hdist = b.get(5)? as usize + 1;
                    let hclen = b.get(4)? as usize + 4;
                    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORDER.iter().take(hclen) {
                        cl[o] = b.get(3)? as u8;
                    }
                    let clh = Huff::new(&cl);
                    let mut lens = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < hlit + hdist {
                        let sym = clh.decode(&mut b)?;
                        match sym {
                            0..=15 => {
                                lens[i] = sym as u8;
                                i += 1;
                            }
                            16 => {
                                let prev = *lens.get(i.checked_sub(1)?)?;
                                for _ in 0..3 + b.get(2)? {
                                    *lens.get_mut(i)? = prev;
                                    i += 1;
                                }
                            }
                            17 => i += 3 + b.get(3)? as usize,
                            _ => i += 11 + b.get(7)? as usize,
                        }
                    }
                    if i > hlit + hdist {
                        return None;
                    }
                    (Huff::new(&lens[..hlit]), Huff::new(&lens[hlit..]))
                };
                loop {
                    let sym = lit.decode(&mut b)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let k = (sym - 257) as usize;
                        let len = *LEN_BASE.get(k)? as usize + b.get(*LEN_EXTRA.get(k)? as u32)? as usize;
                        let dk = dist.decode(&mut b)? as usize;
                        let dd = *DIST_BASE.get(dk)? as usize + b.get(*DIST_EXTRA.get(dk)? as u32)? as usize;
                        if dd > out.len() {
                            return None;
                        }
                        let start = out.len() - dd;
                        for j in 0..len {
                            let v = out[start + j];
                            out.push(v);
                        }
                    }
                }
            }
            _ => return None,
        }
        if last == 1 {
            break;
        }
    }
    Some(out)
}
