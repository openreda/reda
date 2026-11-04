//! Windows `.ico` and macOS `.icns` containers around PNG images, for the app and
//! document icons in `packaging/icons/`.

/// One square PNG image and its edge length in pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Png {
    pub size: u32,
    pub bytes: Vec<u8>,
}

/// A Windows icon holding the PNGs as they are (PNG entries need Vista or later).
pub fn ico(images: &[Png]) -> Vec<u8> {
    let count = u16::try_from(images.len()).expect("an icon holds at most 65535 images");
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for image in images {
        // A 256-pixel edge does not fit a byte, so it is written as 0.
        let edge = u8::try_from(image.size).unwrap_or(0);
        out.extend_from_slice(&[edge, edge, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&len32(image.bytes.len()).to_le_bytes());
        out.extend_from_slice(&len32(offset).to_le_bytes());
        offset += image.bytes.len();
    }
    for image in images {
        out.extend_from_slice(&image.bytes);
    }
    out
}

/// A macOS icon family holding the PNGs, each under the OSType for its size.
pub fn icns(images: &[Png]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    for image in images {
        let ostype: &[u8; 4] = match image.size {
            16 => b"icp4",
            32 => b"icp5",
            64 => b"icp6",
            128 => b"ic07",
            256 => b"ic08",
            512 => b"ic09",
            1024 => b"ic10",
            other => return Err(format!("icns has no image type for {other}×{other}")),
        };
        body.extend_from_slice(ostype);
        body.extend_from_slice(&len32(8 + image.bytes.len()).to_be_bytes());
        body.extend_from_slice(&image.bytes);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&len32(8 + body.len()).to_be_bytes());
    out.extend(body);
    Ok(out)
}

fn len32(len: usize) -> u32 {
    u32::try_from(len).expect("icon files stay far below 4 GiB")
}
