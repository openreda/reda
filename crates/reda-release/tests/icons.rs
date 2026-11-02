use reda_release::{Png, icns, ico};

fn png(size: u32, body: &[u8]) -> Png {
    Png {
        size,
        bytes: body.to_vec(),
    }
}

fn u32_be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}

fn u32_le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn ico_holds_each_png_behind_a_directory_entry() {
    // ICONDIR (reserved 0, type 1 = icon, count) then one 16-byte ICONDIRENTRY per image;
    // a width or height of 0 means 256. Vista and later read PNG-compressed entries.
    let file = ico(&[png(16, b"sixteen"), png(256, b"big")]);
    assert_eq!(&file[0..6], &[0, 0, 1, 0, 2, 0]);

    let first = &file[6..22];
    assert_eq!(
        &first[0..4],
        &[16, 16, 0, 0],
        "width, height, colours, reserved"
    );
    assert_eq!(&first[4..8], &[1, 0, 32, 0], "planes 1, 32 bits per pixel");
    assert_eq!(u32_le(&first[8..12]), 7, "byte length");
    assert_eq!(
        u32_le(&first[12..16]),
        6 + 2 * 16,
        "offset after the directory"
    );

    let second = &file[22..38];
    assert_eq!(&second[0..2], &[0, 0], "256 is written as 0");
    assert_eq!(u32_le(&second[8..12]), 3);
    assert_eq!(u32_le(&second[12..16]), 6 + 2 * 16 + 7);

    assert_eq!(&file[38..], b"sixteenbig");
}

#[test]
fn icns_tags_each_png_with_its_ostype() {
    // 'icns' + total length (big-endian), then per image: OSType + length incl. its 8-byte
    // header + PNG data. 16 → icp4, 32 → icp5, 64 → icp6, 128 → ic07, 256 → ic08, 512 → ic09.
    let file = icns(&[png(16, b"a"), png(512, b"bcd")]).unwrap();
    assert_eq!(&file[0..4], b"icns");
    assert_eq!(u32_be(&file[4..8]) as usize, file.len());
    assert_eq!(&file[8..12], b"icp4");
    assert_eq!(u32_be(&file[12..16]), 9);
    assert_eq!(&file[16..17], b"a");
    assert_eq!(&file[17..21], b"ic09");
    assert_eq!(u32_be(&file[21..25]), 11);
    assert_eq!(&file[25..], b"bcd");
}

#[test]
fn icns_rejects_a_size_it_has_no_type_for() {
    assert!(icns(&[png(48, b"x")]).is_err());
}
