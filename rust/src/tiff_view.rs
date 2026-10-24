//! Read-only TIFF tag view for palette and WhiteIsZero decoding.
//! tiff 0.11 does not expose palette indices and cannot invert gray+alpha.
//! Present those samples as BlackIsZero to the decoder, then apply the original
//! palette/inversion ourselves. No source bytes or pixel data are rewritten.
use std::io::{self, Read, Seek, SeekFrom};

pub(crate) struct TiffView<R> {
    inner: R,
    position: u64,
    patch: Option<(u64, [u8; 2])>,
}
impl<R: Read + Seek> TiffView<R> {
    pub fn new(mut inner: R) -> io::Result<Self> {
        inner.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner,
            position: 0,
            patch: None,
        })
    }
    pub fn black_is_zero(&mut self) -> io::Result<()> {
        let saved = self.position;
        let result = self.locate_photometric();
        self.inner.seek(SeekFrom::Start(saved))?;
        self.position = saved;
        self.patch = Some(result?);
        Ok(())
    }
    fn locate_photometric(&mut self) -> io::Result<(u64, [u8; 2])> {
        let bad = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Ongeldige TIFF-photometric directory",
            )
        };
        self.inner.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 8];
        self.inner.read_exact(&mut header)?;
        let little = match &header[..2] {
            b"II" => true,
            b"MM" => false,
            _ => return Err(bad()),
        };
        let u16_at = |s: &[u8]| {
            if little {
                u16::from_le_bytes(s.try_into().unwrap())
            } else {
                u16::from_be_bytes(s.try_into().unwrap())
            }
        };
        let u32_at = |s: &[u8]| {
            if little {
                u32::from_le_bytes(s.try_into().unwrap())
            } else {
                u32::from_be_bytes(s.try_into().unwrap())
            }
        };
        let u64_at = |s: &[u8]| {
            if little {
                u64::from_le_bytes(s.try_into().unwrap())
            } else {
                u64::from_be_bytes(s.try_into().unwrap())
            }
        };
        let big = match u16_at(&header[2..4]) {
            42 => false,
            43 => true,
            _ => return Err(bad()),
        };
        let offset = if big {
            if u16_at(&header[4..6]) != 8 || u16_at(&header[6..8]) != 0 {
                return Err(bad());
            }
            let mut bytes = [0; 8];
            self.inner.read_exact(&mut bytes)?;
            u64_at(&bytes)
        } else {
            u64::from(u32_at(&header[4..8]))
        };
        self.inner.seek(SeekFrom::Start(offset))?;
        let count = if big {
            let mut b = [0; 8];
            self.inner.read_exact(&mut b)?;
            u64_at(&b)
        } else {
            let mut b = [0; 2];
            self.inner.read_exact(&mut b)?;
            u64::from(u16_at(&b))
        };
        if count == 0 || count > 65_535 {
            return Err(bad());
        }
        let entry_len = if big { 20 } else { 12 };
        let base = offset
            .checked_add(if big { 8 } else { 2 })
            .ok_or_else(bad)?;
        for i in 0..count {
            let entry_offset = base.checked_add(i * entry_len).ok_or_else(bad)?;
            self.inner.seek(SeekFrom::Start(entry_offset))?;
            let mut bytes = [0; 20];
            self.inner.read_exact(&mut bytes[..entry_len as usize])?;
            if u16_at(&bytes[..2]) != 262 {
                continue;
            }
            let n = if big {
                u64_at(&bytes[4..12])
            } else {
                u64::from(u32_at(&bytes[4..8]))
            };
            if u16_at(&bytes[2..4]) != 3 || n != 1 {
                return Err(bad());
            }
            return Ok((
                entry_offset + if big { 12 } else { 8 },
                if little {
                    1u16.to_le_bytes()
                } else {
                    1u16.to_be_bytes()
                },
            ));
        }
        Err(bad())
    }
}
impl<R: Read> Read for TiffView<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buf)?;
        if let Some((offset, bytes)) = self.patch {
            for (i, byte) in bytes.into_iter().enumerate() {
                if let Some(index) = (offset + i as u64)
                    .checked_sub(self.position)
                    .filter(|v| *v < count as u64)
                {
                    buf[index as usize] = byte;
                }
            }
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl<R: Seek> Seek for TiffView<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.position = self.inner.seek(pos)?;
        Ok(self.position)
    }
}
