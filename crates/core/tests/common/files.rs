//! What the integration tests of the file-reading area share: a folder of the test's own, PNG files written byte by
//! byte (so each expected level comes from the PNG standard, not from an encoder), and two tiny JPEGs no encoder here
//! can write.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A folder of the test's own, removed when it goes out of scope.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new() -> std::io::Result<Self> {
        static FOLDERS: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bt-image-files-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    /// The path of the file `name` in the folder, which need not exist.
    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// Writes `content` to the file `name` and gives its path.
    pub(crate) fn write(&self, name: &str, content: impl AsRef<[u8]>) -> std::io::Result<PathBuf> {
        let path = self.path(name);
        std::fs::write(&path, content)?;
        Ok(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The CRC-32 of PNG (ISO 3309), bit by bit.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The Adler-32 checksum that ends a zlib stream (RFC 1950).
fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// The PNG chunk of type `kind` holding `data`: length, type, data, CRC.
pub(crate) fn chunk(kind: &str, data: &[u8]) -> Vec<u8> {
    let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
    let mut bytes = length.to_be_bytes().to_vec();
    let body = [kind.as_bytes(), data].concat();
    bytes.extend_from_slice(&body);
    bytes.extend_from_slice(&crc32(&body).to_be_bytes());
    bytes
}

/// The eight bytes every PNG file starts with.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// A PNG file written by hand: the header, the chunks that go before the pixels, the pixels, the chunks after them.
pub(crate) struct Png {
    width: u32,
    height: u32,
    depth: u8,
    colour_type: u8,
    before: Vec<Vec<u8>>,
    rows: Vec<Vec<u8>>,
    after: Vec<Vec<u8>>,
}

impl Png {
    /// A `width` x `height` file of `depth` bits per sample and PNG `colour_type` (0 grey, 2 RGB, 3 palette, 4 grey
    /// with alpha, 6 RGBA) whose `rows` are the bytes of each row, samples packed as the standard says.
    pub(crate) fn new(width: u32, height: u32, depth: u8, colour_type: u8, rows: &[&[u8]]) -> Self {
        Self {
            width,
            height,
            depth,
            colour_type,
            before: Vec::new(),
            rows: rows.iter().map(|row| row.to_vec()).collect(),
            after: Vec::new(),
        }
    }

    /// This file with `chunk` (see [`chunk`]) before its pixels: `PLTE`, `tRNS`, `eXIf`, `acTL`, `fcTL`.
    pub(crate) fn before(mut self, chunk: Vec<u8>) -> Self {
        self.before.push(chunk);
        self
    }

    /// This file with `chunk` after its pixels: an animation's later frames.
    pub(crate) fn after(mut self, chunk: Vec<u8>) -> Self {
        self.after.push(chunk);
        self
    }

    /// The pixels as the zlib stream a pixel chunk holds: one stored (uncompressed) deflate block, filter 0 on every row.
    pub(crate) fn pixels(&self) -> Vec<u8> {
        let raw: Vec<u8> = self.rows.iter().flat_map(|row| [&[0_u8][..], row].concat()).collect();
        let length = u16::try_from(raw.len()).unwrap_or(u16::MAX);
        let mut zlib = vec![0x78, 0x01, 0x01];
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(&raw);
        zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
        zlib
    }

    /// The file's bytes.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        let mut header = [self.width.to_be_bytes(), self.height.to_be_bytes()].concat();
        header.extend_from_slice(&[self.depth, self.colour_type, 0, 0, 0]);
        let mut bytes = SIGNATURE.to_vec();
        bytes.extend(chunk("IHDR", &header));
        bytes.extend(self.before.iter().flatten());
        bytes.extend(chunk("IDAT", &self.pixels()));
        bytes.extend(self.after.iter().flatten());
        bytes.extend(chunk("IEND", &[]));
        bytes
    }
}

/// The start of a PNG file that claims `width` x `height` pixels of 8-bit grey and holds none: the signature, the header
/// and an empty pixel chunk. Enough for a reader to learn the size, which is all a limit on size looks at.
pub(crate) fn png_claiming(width: u32, height: u32) -> Vec<u8> {
    let mut header = [width.to_be_bytes(), height.to_be_bytes()].concat();
    header.extend_from_slice(&[8, 0, 0, 0, 0]);
    let mut bytes = SIGNATURE.to_vec();
    bytes.extend(chunk("IHDR", &header));
    bytes.extend(chunk("IDAT", &[]));
    bytes
}

/// An EXIF block (the TIFF structure a PNG `eXIf` chunk holds) whose only tag is the orientation `value`.
pub(crate) fn exif(orientation: u16) -> Vec<u8> {
    let mut tiff = vec![b'I', b'I', 42, 0, 8, 0, 0, 0]; // little endian, the first directory at byte 8
    tiff.extend_from_slice(&1_u16.to_le_bytes()); // one entry
    tiff.extend_from_slice(&0x0112_u16.to_le_bytes()); // tag Orientation
    tiff.extend_from_slice(&3_u16.to_le_bytes()); // type SHORT
    tiff.extend_from_slice(&1_u32.to_le_bytes()); // one value
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0; 2]); // the rest of the four bytes of the value
    tiff.extend_from_slice(&[0; 4]); // no next directory
    tiff
}

/// The bytes a string of hexadecimal digits spells.
fn unhex(digits: &str) -> Vec<u8> {
    digits
        .as_bytes()
        .chunks(2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

/// An 8 x 8 progressive JPEG of one colour, 200, 100, 50 (red, green, blue), made once with Pillow
/// (`quality=100, subsampling=0, progressive=True`) because the encoders here write baseline files only.
pub(crate) fn progressive_jpeg() -> Vec<u8> {
    unhex(
        &[
            "ffd8ffe000104a46494600010100000100010000ffdb0043000101010101010101010101010101010101010101010101",
            "0101010101010101010101010101010101010101010101010101010101010101010101010101010101ffdb0043010101",
            "010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101",
            "0101010101010101010101010101ffc20011080008000803011100021101031101ffc400140001000000000000000000",
            "00000000000005ffc40014010100000000000000000000000000000008ffda000c030100021003100000013caed8ffc4",
            "0014100100000000000000000000000000000000ffda00080101000105027fffc4001411010000000000000000000000",
            "0000000000ffda0008010301013f017fffc40014110100000000000000000000000000000000ffda0008010201013f01",
            "7fffc40014100100000000000000000000000000000000ffda0008010100063f027fffc4001410010000000000000000",
            "0000000000000000ffda0008010100013f217fffda000c030100020003000000101fffc4001411010000000000000000",
            "0000000000000000ffda0008010301013f107fffc40014110100000000000000000000000000000000ffda0008010201",
            "013f107fffc40014100100000000000000000000000000000000ffda0008010100013f107fffd9",
        ]
        .concat(),
    )
}

/// An 8 x 8 CMYK JPEG (Adobe's: four components, inks stored inverted) of one colour, cyan 0, magenta 255, yellow 255,
/// black 0, which with no profile is pure red; made once with Pillow (`Image.fromarray(inks, "CMYK").save(quality=100)`).
pub(crate) fn cmyk_jpeg() -> Vec<u8> {
    unhex(
        &[
            "ffd8ffee000e41646f626500640000000000ffdb00430001010101010101010101010101010101010101010101010101",
            "010101010101010101010101010101010101010101010101010101010101010101010101010101ffc000140800080008",
            "044311004d11005911004b1100ffc4001f0000010501010101010100000000000000000102030405060708090a0bffc4",
            "00b5100002010303020403050504040000017d01020300041105122131410613516107227114328191a1082342b1c115",
            "52d1f02433627282090a161718191a25262728292a3435363738393a434445464748494a535455565758595a63646566",
            "6768696a737475767778797a838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2",
            "c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1f2f3f4f5f6f7f8f9faffda000e0443004d005900",
            "4b00003f00fefe2bfcff00ebfcff00ebfbf8afffd9",
        ]
        .concat(),
    )
}

/// A little-endian TIFF of one 32-bit sample per pixel, `width` x `height`, in one strip: `sample_format` is the TIFF
/// tag (1 unsigned integers, 2 signed integers, 3 floats), `samples` the bytes of each pixel's level, row after row,
/// and `orientation` the EXIF orientation to tag it with, if any.
pub(crate) fn tiff_wide(
    width: u32,
    height: u32,
    sample_format: u16,
    orientation: Option<u16>,
    samples: &[[u8; 4]],
) -> Vec<u8> {
    let entry = |tag: u16, kind: u16, value: u32| {
        [
            tag.to_le_bytes().as_slice(),
            &kind.to_le_bytes(),
            &1_u32.to_le_bytes(),
            &value.to_le_bytes(),
        ]
        .concat()
    };
    let (short, long) = (3, 4);
    let mut entries = vec![
        entry(256, long, width),
        entry(257, long, height),
        entry(258, short, 32), // bits per sample
        entry(262, short, 1),  // black is zero
    ];
    let data_at = 8 + 2 + 12 * (9 + u32::from(orientation.is_some())) + 4;
    entries.push(entry(273, long, data_at)); // strip offset
    if let Some(orientation) = orientation {
        entries.push(entry(274, short, u32::from(orientation)));
    }
    entries.extend([
        entry(277, short, 1), // samples per pixel
        entry(278, long, height),
        entry(279, long, u32::try_from(samples.len() * 4).unwrap_or(u32::MAX)), // strip byte count
        entry(339, short, u32::from(sample_format)),
    ]);
    let mut file = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
    file.extend_from_slice(&u16::try_from(entries.len()).unwrap_or(u16::MAX).to_le_bytes());
    file.extend(entries.concat());
    file.extend_from_slice(&[0; 4]); // no next directory
    file.extend(samples.concat());
    file
}

/// A GIF 35 bytes long whose screen is 1 x 1 pixel and whose only frame is 65535 x 65535: a decoder that trusts the frame
/// allocates 17 GB for it. The frame holds two bytes of compressed data.
pub(crate) fn gif_frame_larger_than_its_screen() -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0]); // screen 1 x 1, no colour table, background 0, aspect 0
    bytes.push(0x2C); // an image
    bytes.extend_from_slice(&[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]); // at 0, 0; 65535 x 65535
    bytes.push(0x80); // a colour table of 2 entries follows
    bytes.extend_from_slice(&[0, 0, 0, 0xFF, 0xFF, 0xFF]);
    bytes.extend_from_slice(&[2, 2, 0x4C, 0x01, 0]); // LZW minimum code size 2, one block of 2 bytes, end of data
    bytes.push(0x3B); // end of file
    bytes
}
