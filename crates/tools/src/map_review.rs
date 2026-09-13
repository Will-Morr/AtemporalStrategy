//! `map-review`: renders one row per seed with the cave, raw traffic, smoothed traffic and the
//! final layout with ore, as a PNG written without external crates (stored deflate blocks).
use atemporal_sim::*;

const SCALE: usize = 5;
const GAP: usize = 6;

pub fn render(config: &MatchConfig, content: &Content, count: u64) -> Result<Vec<u8>> {
    let size = usize::from(config.map_size);
    let panel = size * SCALE;
    let width = 4 * panel + 5 * GAP;
    let height = count as usize * (panel + GAP) + GAP;
    let mut img = Image {
        width,
        height,
        rgb: vec![0x12; width * height * 3],
    };
    for row in 0..count {
        let mut config = config.clone();
        config.seed = row.try_into()?;
        let starts = map::starts(config.map_size, config.player_count, content)?;
        let terrain = map::terrain(&config, &starts)?;
        let traffic = map::traffic(&config, &terrain, &starts);
        let ore = map::ore_tiles(&config, &terrain, &starts, &traffic);
        let peak_raw = f64::from(*traffic.raw.iter().max().unwrap_or(&1)).max(1.0);
        let peak_smooth = traffic.smooth.iter().cloned().fold(0.0, f64::max).max(1.0);
        let oy = GAP + row as usize * (panel + GAP);
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                let floor = terrain.cells[i] == TerrainCell::Floor;
                let base = if floor { (150, 150, 160) } else { (40, 40, 48) };
                let raw = heat(f64::from(traffic.raw[i]) / peak_raw, floor);
                let smooth = heat(traffic.smooth[i] / peak_smooth, floor);
                let final_ = if ore.contains(&Tile {
                    x: x as u16,
                    y: y as u16,
                }) {
                    (240, 190, 40)
                } else {
                    base
                };
                for (col, color) in [base, raw, smooth, final_].into_iter().enumerate() {
                    img.block(GAP + col * (panel + GAP) + x * SCALE, oy + y * SCALE, color);
                }
            }
        }
        for (player, s) in starts.iter().enumerate() {
            let color = [
                (80, 160, 255),
                (255, 90, 90),
                (90, 220, 120),
                (230, 120, 255),
            ][player % 4];
            for (_, t, _) in &s.entities {
                for col in [0, 3] {
                    img.block(
                        GAP + col * (panel + GAP) + usize::from(t.x) * SCALE,
                        oy + usize::from(t.y) * SCALE,
                        color,
                    );
                }
            }
        }
    }
    Ok(img.png())
}

/// Dark floor through yellow to red as heat rises; rock stays dark.
fn heat(v: f64, floor: bool) -> (u8, u8, u8) {
    if !floor {
        return (40, 40, 48);
    }
    let v = v.clamp(0.0, 1.0);
    let lerp = |a: f64, b: f64, t: f64| (a + (b - a) * t) as u8;
    if v < 0.5 {
        let t = v * 2.0;
        (
            lerp(70.0, 240.0, t),
            lerp(70.0, 200.0, t),
            lerp(80.0, 40.0, t),
        )
    } else {
        let t = (v - 0.5) * 2.0;
        (240, lerp(200.0, 40.0, t), 40)
    }
}

struct Image {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl Image {
    fn block(&mut self, x: usize, y: usize, (r, g, b): (u8, u8, u8)) {
        for dy in 0..SCALE {
            for dx in 0..SCALE {
                let i = ((y + dy) * self.width + x + dx) * 3;
                self.rgb[i..i + 3].copy_from_slice(&[r, g, b]);
            }
        }
    }

    fn png(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity(self.height * (self.width * 3 + 1));
        for y in 0..self.height {
            raw.push(0);
            raw.extend_from_slice(&self.rgb[y * self.width * 3..(y + 1) * self.width * 3]);
        }
        let mut idat = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
        for (i, block) in blocks.iter().enumerate() {
            idat.push(u8::from(i + 1 == blocks.len()));
            let len = block.len() as u16;
            idat.extend_from_slice(&len.to_le_bytes());
            idat.extend_from_slice(&(!len).to_le_bytes());
            idat.extend_from_slice(block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        idat.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(self.width as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.height as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
        for (kind, data) in [(b"IHDR", ihdr), (b"IDAT", idat), (b"IEND", vec![])] {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(&data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc32(&body).to_be_bytes());
        }
        out
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
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
