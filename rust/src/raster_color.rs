use tiff::decoder::DecodingResult;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Sample {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}
impl Sample {
    pub fn value(self) -> f64 {
        match self {
            Self::Unsigned(v) => v as f64,
            Self::Signed(v) => v as f64,
            Self::Float(v) => v,
        }
    }
    fn invert(self, bits: u8) -> Self {
        match self {
            Self::Unsigned(v) => Self::Unsigned(maximum(bits) - v),
            Self::Signed(v) => Self::Signed(((maximum(bits) >> 1) as i64).saturating_sub(v)),
            Self::Float(v) => Self::Float(1.0 - v),
        }
    }
    fn byte(self, bits: u8, gray: bool) -> u8 {
        match self {
            Self::Unsigned(v) if gray && bits >= 8 => v.min(255) as u8,
            Self::Unsigned(v) if bits < 8 => ((v * 255) / maximum(bits)) as u8,
            Self::Unsigned(v) => (v >> (bits - 8)) as u8,
            Self::Signed(v) => v.clamp(0, 255) as u8,
            Self::Float(v) if gray => v.clamp(0.0, 255.0) as u8,
            Self::Float(v) => (v.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
    fn unassociate(self, alpha: Self, bits: u8) -> Self {
        match (self, alpha) {
            (Self::Unsigned(c), Self::Unsigned(a)) => Self::Unsigned(if a == 0 {
                0
            } else {
                ((u128::from(c) * u128::from(maximum(bits))) / u128::from(a))
                    .min(u128::from(maximum(bits))) as u64
            }),
            _ => Self::Float(if alpha.value() <= 0.0 {
                0.0
            } else {
                self.value() / alpha.value()
            }),
        }
    }
}
fn maximum(bits: u8) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

pub(crate) struct Chunk {
    pub data: DecodingResult,
    pub row_stride: usize,
}
impl Chunk {
    pub fn sample(
        &self,
        x: usize,
        y: usize,
        channel: usize,
        channels: usize,
        bits: u8,
    ) -> Result<Sample, String> {
        let bit = (x * channels + channel) * usize::from(bits);
        let byte = y * self.row_stride + bit / 8;
        let bad = || "TIFF-strip/tegel bevat minder pixels dan verwacht".to_owned();
        if bits < 8 {
            let DecodingResult::U8(v) = &self.data else {
                return Err(bad());
            };
            let n = (*v.get(byte).ok_or_else(bad)? >> (8 - usize::from(bits) - (bit % 8)))
                & ((1u8 << bits) - 1);
            return Ok(Sample::Unsigned(u64::from(n)));
        }
        let index = byte / (usize::from(bits) / 8);
        macro_rules! at {
            ($v:expr,$kind:ident,$convert:expr) => {
                $v.get(index)
                    .map(|v| Sample::$kind($convert(*v)))
                    .ok_or_else(bad)
            };
        }
        match &self.data {
            DecodingResult::U8(v) => at!(v, Unsigned, u64::from),
            DecodingResult::U16(v) => at!(v, Unsigned, u64::from),
            DecodingResult::U32(v) => at!(v, Unsigned, u64::from),
            DecodingResult::U64(v) => at!(v, Unsigned, |v| v),
            DecodingResult::I8(v) => at!(v, Signed, i64::from),
            DecodingResult::I16(v) => at!(v, Signed, i64::from),
            DecodingResult::I32(v) => at!(v, Signed, i64::from),
            DecodingResult::I64(v) => at!(v, Signed, |v| v),
            DecodingResult::F16(v) => v
                .get(index)
                .map(|v| Sample::Float(v.to_f64()))
                .ok_or_else(bad),
            DecodingResult::F32(v) => at!(v, Float, f64::from),
            DecodingResult::F64(v) => at!(v, Float, |v| v),
        }
    }
}

pub(crate) struct Colors {
    pub photometric: u32,
    pub bits: u8,
    pub channels: usize,
    pub planar: bool,
    pub alpha: Option<usize>,
    pub associated: bool,
    pub palette: Vec<u16>,
    pub nodata: Option<f64>,
    pub ycbcr_coefficients: [f64; 3],
    pub ycbcr_reference: [f64; 6],
}
impl Colors {
    pub fn required_planes(&self) -> usize {
        if !self.planar {
            return 1;
        }
        let base = match self.photometric {
            0 | 1 | 3 => 1,
            2 | 6 => 3,
            5 => 4,
            _ => 0,
        };
        self.alpha.map_or(base, |a| base.max(a + 1))
    }
    pub fn rgba(&self, chunks: &[Chunk], x: usize, y: usize) -> Result<[u8; 4], String> {
        let get = |c: usize| {
            if self.planar {
                chunks[c].sample(x, y, 0, 1, self.bits)
            } else {
                chunks[0].sample(x, y, c, self.channels, self.bits)
            }
        };
        let base = match self.photometric {
            0 | 1 | 3 => 1,
            2 | 6 => 3,
            5 => 4,
            _ => return Err("TIFF-kleurruimte wordt niet ondersteund".into()),
        };
        let mut raw = [Sample::Unsigned(0); 4];
        for (c, slot) in raw.iter_mut().enumerate().take(base) {
            *slot = get(c)?;
        }
        if raw[..base].iter().any(|s| !s.value().is_finite())
            || self
                .nodata
                .is_some_and(|n| raw[..base].iter().all(|s| s.value() == n))
        {
            return Ok([0; 4]);
        }
        let alpha = self.alpha.map(get).transpose()?;
        if alpha.is_some_and(|a| !a.value().is_finite()) {
            return Ok([0; 4]);
        }
        let a = alpha.map_or(255, |a| a.byte(self.bits, false));
        if self.associated && self.photometric != 2 {
            if let Some(alpha) = alpha {
                for sample in &mut raw[..base] {
                    *sample = sample.unassociate(alpha, self.bits);
                }
            }
        }
        let rgb = match self.photometric {
            0 | 1 => {
                let v = if self.photometric == 0 {
                    raw[0].invert(self.bits)
                } else {
                    raw[0]
                }
                .byte(self.bits, true);
                [v, v, v]
            }
            2 => {
                let mut rgb = [
                    raw[0].byte(self.bits, false),
                    raw[1].byte(self.bits, false),
                    raw[2].byte(self.bits, false),
                ];
                // Pillow's RGB(A) high-depth loader first reduces samples to 8
                // bits, then unassociates. Keep this order for pixel parity.
                if self.associated {
                    for v in &mut rgb {
                        *v = if a == 0 {
                            0
                        } else {
                            (u32::from(*v) * 255 / u32::from(a)).min(255) as u8
                        };
                    }
                }
                rgb
            }
            3 => {
                let index = raw[0].value() as usize;
                let count = 1usize << self.bits;
                if index >= count || self.palette.len() != count * 3 {
                    return Err("Ongeldige TIFF-kleurenkaart/index".into());
                }
                [
                    (self.palette[index] >> 8) as u8,
                    (self.palette[count + index] >> 8) as u8,
                    (self.palette[2 * count + index] >> 8) as u8,
                ]
            }
            5 => {
                let k = u32::from(255 - raw[3].byte(self.bits, false));
                [0, 1, 2].map(|i| {
                    ((u32::from(255 - raw[i].byte(self.bits, false)) * k + 127) / 255) as u8
                })
            }
            6 => {
                let r = self.ycbcr_reference;
                let y = (raw[0].value() - r[0]) * 255.0 / (r[1] - r[0]);
                let cb = (raw[1].value() - r[2]) * 127.0 / (r[3] - r[2]);
                let cr = (raw[2].value() - r[4]) * 127.0 / (r[5] - r[4]);
                let [kr, kg, kb] = self.ycbcr_coefficients;
                [
                    y + cr * (2.0 - 2.0 * kr),
                    y - cb * kb * (2.0 - 2.0 * kb) / kg - cr * kr * (2.0 - 2.0 * kr) / kg,
                    y + cb * (2.0 - 2.0 * kb),
                ]
                .map(|v| v.round().clamp(0.0, 255.0) as u8)
            }
            _ => unreachable!(),
        };
        Ok([rgb[0], rgb[1], rgb[2], a])
    }
}
