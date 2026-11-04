//! A tiny software rasteriser: draws filled shapes into an 8-bit alpha
//! canvas. It exists to generate tile art without any graphics library.

/// A rectangle of alpha values (0 = empty, 255 = solid)
pub struct AlphaCanvas {
    width: usize,
    height: usize,
    alpha: Vec<u8>,
}

impl AlphaCanvas {
    pub fn new(width: usize, height: usize) -> Self {
        AlphaCanvas { width, height, alpha: vec![0; width * height] }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The alpha channel of an RGBA picture (4 bytes per pixel)
    pub fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Self {
        assert_eq!(rgba.len(), width * height * 4, "RGBA data is the wrong size");
        AlphaCanvas {
            width,
            height,
            alpha: rgba.chunks_exact(4).map(|px| px[3]).collect(),
        }
    }

    pub fn alpha(&self) -> &[u8] {
        &self.alpha
    }

    /// Alpha at (x, y); 0 outside the canvas
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.alpha[y as usize * self.width + x as usize]
    }

    /// Set one pixel; anything outside the canvas is ignored
    pub fn set(&mut self, x: i32, y: i32, value: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        self.alpha[y as usize * self.width + x as usize] = value;
    }

    pub fn fill(&mut self, value: u8) {
        for a in self.alpha.iter_mut() {
            *a = value;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, value: u8) {
        for py in y..y + h {
            for px in x..x + w {
                self.set(px, py, value);
            }
        }
    }

    /// A one-pixel line (Bresenham), end points included
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, value: u8) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let (mut x, mut y) = (x0, y0);
        let mut err = dx + dy;
        loop {
            self.set(x, y, value);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// A filled polygon: every pixel whose middle is inside it or exactly on
    /// its edge. Treating the edge as inside keeps the result identical when
    /// the shape is rotated or mirrored.
    pub fn fill_polygon(&mut self, points: &[(i32, i32)], value: u8) {
        if points.len() < 3 {
            return;
        }
        const EDGE_SLACK: f64 = 1e-9;
        let min_y = points.iter().map(|p| p.1).min().unwrap();
        let max_y = points.iter().map(|p| p.1).max().unwrap();

        for y in min_y..max_y {
            // Row middles are at .5 and corners are whole numbers, so a row
            // never passes exactly through a corner
            let yc = y as f64 + 0.5;
            let mut crossings: Vec<f64> = Vec::new();
            for i in 0..points.len() {
                let (x0, y0) = points[i];
                let (x1, y1) = points[(i + 1) % points.len()];
                if y0 == y1 {
                    continue;
                }
                let (top, bottom) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                if yc > top as f64 && yc < bottom as f64 {
                    let t = (yc - y0 as f64) / (y1 - y0) as f64;
                    crossings.push(x0 as f64 + t * (x1 - x0) as f64);
                }
            }
            crossings.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in crossings.chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let first = (pair[0] - 0.5 - EDGE_SLACK).ceil() as i32;
                let last = (pair[1] - 0.5 + EDGE_SLACK).floor() as i32;
                for x in first..=last {
                    self.set(x, y, value);
                }
            }
        }
    }

    /// A filled circle. The centre is a point on the pixel grid (a pixel
    /// corner) and a pixel is filled when its middle is within `radius` of
    /// it, so mirrored circles always come out identical.
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, value: u8) {
        let r2 = (radius * radius) as f64;
        for y in cy - radius..cy + radius {
            for x in cx - radius..cx + radius {
                let dx = x as f64 + 0.5 - cx as f64;
                let dy = y as f64 + 0.5 - cy as f64;
                if dx * dx + dy * dy <= r2 {
                    self.set(x, y, value);
                }
            }
        }
    }

    /// Resize to `new_width` x `new_height`, each new pixel being the average
    /// of the part of the old picture it covers (a box filter: it smooths
    /// when shrinking and is blocky when enlarging)
    pub fn scale_to(&self, new_width: usize, new_height: usize) -> AlphaCanvas {
        let mut out = AlphaCanvas::new(new_width, new_height);
        let sx = self.width as f64 / new_width as f64;
        let sy = self.height as f64 / new_height as f64;
        for y in 0..new_height {
            let (y0, y1) = (y as f64 * sy, (y + 1) as f64 * sy);
            for x in 0..new_width {
                let (x0, x1) = (x as f64 * sx, (x + 1) as f64 * sx);
                let mut sum = 0.0;
                let mut weight = 0.0;
                for py in (y0.floor() as usize)..(y1.ceil() as usize).min(self.height) {
                    let wy = (y1.min((py + 1) as f64) - y0.max(py as f64)).max(0.0);
                    for px in (x0.floor() as usize)..(x1.ceil() as usize).min(self.width) {
                        let wx = (x1.min((px + 1) as f64) - x0.max(px as f64)).max(0.0);
                        sum += self.alpha[py * self.width + px] as f64 * wx * wy;
                        weight += wx * wy;
                    }
                }
                out.alpha[y * new_width + x] = if weight > 0.0 { (sum / weight).round() as u8 } else { 0 };
            }
        }
        out
    }

    /// Turn clockwise by `angle` radians about the centre. The result is
    /// just big enough to hold the whole turned picture; corners that the
    /// picture does not reach are empty.
    pub fn rotate(&self, angle: f64) -> AlphaCanvas {
        const SIZE_SLACK: f64 = 1e-9; // so exactly 90 degrees does not add a pixel
        let (sin, cos) = angle.sin_cos();
        let (w, h) = (self.width as f64, self.height as f64);
        let new_w = ((w * cos.abs() + h * sin.abs()) - SIZE_SLACK).ceil().max(1.0) as usize;
        let new_h = ((w * sin.abs() + h * cos.abs()) - SIZE_SLACK).ceil().max(1.0) as usize;
        let mut out = AlphaCanvas::new(new_w, new_h);
        for y in 0..new_h {
            for x in 0..new_w {
                // this pixel's middle, relative to the new centre, turned
                // back to find where it came from in the old picture
                let dx = x as f64 + 0.5 - new_w as f64 / 2.0;
                let dy = y as f64 + 0.5 - new_h as f64 / 2.0;
                let src_x = dx * cos + dy * sin + w / 2.0 - 0.5;
                let src_y = -dx * sin + dy * cos + h / 2.0 - 0.5;
                out.alpha[y * new_w + x] = self.sample(src_x, src_y);
            }
        }
        out
    }

    /// Alpha at a position between pixels (pixel middles are whole numbers),
    /// blended from the four nearest; outside the picture counts as empty
    fn sample(&self, x: f64, y: f64) -> u8 {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let a = self.get(x0, y0) as f64;
        let b = self.get(x0 + 1, y0) as f64;
        let c = self.get(x0, y0 + 1) as f64;
        let d = self.get(x0 + 1, y0 + 1) as f64;
        let value = a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy)
            + c * (1.0 - fx) * fy + d * fx * fy;
        value.round() as u8
    }

    /// Shrink by an integer factor, averaging each factor x factor block
    pub fn downsample(&self, factor: usize) -> AlphaCanvas {
        let mut out = AlphaCanvas::new(self.width / factor, self.height / factor);
        let n = (factor * factor) as u32;
        for y in 0..out.height {
            for x in 0..out.width {
                let mut sum = 0u32;
                for sy in 0..factor {
                    for sx in 0..factor {
                        sum += self.alpha[(y * factor + sy) * self.width + x * factor + sx] as u32;
                    }
                }
                out.alpha[y * out.width + x] = ((sum + n / 2) / n) as u8;
            }
        }
        out
    }
}
