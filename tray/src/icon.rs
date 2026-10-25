/// A decoded icon in the RGBA byte order consumed by iced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconPixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl IconPixmap {
    /// Convert a big-endian ARGB32 icon into RGBA bytes.
    pub fn from_be_bytes(width: i32, height: i32, raw: &[u8]) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if raw.len() < expected {
            return None;
        }

        let mut data = raw[..expected].to_vec();
        for pixel in data.chunks_exact_mut(4) {
            pixel.rotate_right(1);
        }

        Some(Self {
            width: width as u32,
            height: height as u32,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_argb32be_to_rgba() {
        let pixmap = IconPixmap::from_be_bytes(1, 1, &[0xff, 0x11, 0x22, 0x33]).unwrap();
        assert_eq!(pixmap.width, 1);
        assert_eq!(pixmap.height, 1);
        assert_eq!(pixmap.data, vec![0x11, 0x22, 0x33, 0xff]);
    }

    #[test]
    fn rejects_invalid_pixmaps() {
        assert!(IconPixmap::from_be_bytes(0, 1, &[]).is_none());
        assert!(IconPixmap::from_be_bytes(1, 1, &[0; 3]).is_none());
    }
}
