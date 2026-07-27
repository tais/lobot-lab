#[derive(Clone, Debug)]
pub struct StiSubImage {
    pub offset_x: i16,
    pub offset_y: i16,
    pub width: u16,
    pub height: u16,
    pub indices: Vec<u8>,
    pub alpha: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct StiImage {
    pub palette: Vec<u8>,
    pub subimages: Vec<StiSubImage>,
}

const PALETTE_OFFSET: usize = 64;
const PALETTE_LENGTH: usize = 256 * 3;
const SUBIMAGE_TABLE_OFFSET: usize = PALETTE_OFFSET + PALETTE_LENGTH;
const SUBIMAGE_ENTRY_LENGTH: usize = 16;
const STCI_INDEXED: u32 = 0x08;

pub fn decode(bytes: &[u8]) -> Result<StiImage, String> {
    if bytes.len() < SUBIMAGE_TABLE_OFFSET || bytes.get(0..4) != Some(b"STCI") {
        return Err("Not an indexed STCI image".into());
    }
    let flags = read_u32(bytes, 16)?;
    if flags & STCI_INDEXED == 0 {
        return Err("Only indexed STI images are currently supported".into());
    }
    let count = read_u16(bytes, 28)? as usize;
    let pixel_offset = SUBIMAGE_TABLE_OFFSET
        .checked_add(count.saturating_mul(SUBIMAGE_ENTRY_LENGTH))
        .ok_or("Invalid STI subimage count")?;
    if pixel_offset > bytes.len() {
        return Err("Truncated STI subimage table".into());
    }

    let palette = bytes[PALETTE_OFFSET..PALETTE_OFFSET + PALETTE_LENGTH].to_vec();
    let mut subimages = Vec::with_capacity(count);
    for index in 0..count {
        let entry = SUBIMAGE_TABLE_OFFSET + index * SUBIMAGE_ENTRY_LENGTH;
        let data_offset = read_u32(bytes, entry)? as usize;
        let offset_x = read_i16(bytes, entry + 8)?;
        let offset_y = read_i16(bytes, entry + 10)?;
        let height = read_u16(bytes, entry + 12)?;
        let width = read_u16(bytes, entry + 14)?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            subimages.push(StiSubImage {
                offset_x,
                offset_y,
                width: 0,
                height: 0,
                indices: Vec::new(),
                alpha: Vec::new(),
            });
            continue;
        }
        let (indices, alpha) = decode_etrle(
            bytes,
            pixel_offset.saturating_add(data_offset),
            width as usize,
            height as usize,
        );
        subimages.push(StiSubImage {
            offset_x,
            offset_y,
            width,
            height,
            indices,
            alpha,
        });
    }
    Ok(StiImage { palette, subimages })
}

pub fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("Could not create PNG header: {error}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|error| format!("Could not encode PNG: {error}"))?;
    }
    Ok(output)
}

fn decode_etrle(src: &[u8], start: usize, width: usize, height: usize) -> (Vec<u8>, Vec<u8>) {
    let mut indices = vec![0; width * height];
    let mut alpha = vec![0; width * height];
    let mut cursor = start;
    for y in 0..height {
        let mut x = 0usize;
        loop {
            let Some(&control) = src.get(cursor) else {
                return (indices, alpha);
            };
            cursor += 1;
            if control == 0 {
                break;
            }
            let length = (control & 0x7f) as usize;
            if control & 0x80 != 0 {
                x += length;
                continue;
            }
            if cursor + length > src.len() {
                return (indices, alpha);
            }
            for offset in 0..length {
                if x < width {
                    let target = y * width + x;
                    indices[target] = src[cursor + offset];
                    alpha[target] = 255;
                    x += 1;
                }
            }
            cursor += length;
        }
    }
    (indices, alpha)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "Truncated STI header".to_string())?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_i16(bytes: &[u8], offset: usize) -> Result<i16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "Truncated STI header".to_string())?;
    Ok(i16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "Truncated STI header".to_string())?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_transparent_and_literal_runs() {
        let (indices, alpha) = decode_etrle(&[0x81, 0x02, 7, 8, 0], 0, 3, 1);
        assert_eq!(indices, [0, 7, 8]);
        assert_eq!(alpha, [0, 255, 255]);
    }
}
