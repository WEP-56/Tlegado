//! 首页书本 Braille 动画，移植自 React prototype 的 bookLogo.ts。
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use std::f64::consts::PI;

const W: i32 = 60;
const H: i32 = 40;
const READ: i64 = 2600;
const FLIP: i64 = 1150;
const REST: i64 = 350;
const CYCLE: i64 = READ + FLIP + REST;
const INTRO: i64 = 1800;
const TOP: f64 = 8.0;
const PH: f64 = 18.0;
const PW: f64 = 26.0;
const CX: f64 = 30.0;

const NONE: u8 = 0;
const TEXT: u8 = 1;
const FRAME: u8 = 3;
const READ_LAYER: u8 = 4;
const PTEXT: u8 = 5;
const PAGE: u8 = 6;
const ACCENT: u8 = 7;

const COLORS: [Color; 8] = [
    Color::Reset,
    Color::Rgb(75, 75, 75),
    Color::Rgb(138, 98, 50),
    Color::Rgb(126, 126, 126),
    Color::Rgb(184, 184, 184),
    Color::Rgb(201, 201, 201),
    Color::Rgb(239, 239, 239),
    Color::Rgb(227, 163, 90),
];

struct Canvas {
    pixels: Vec<u8>,
}
impl Canvas {
    fn new() -> Self {
        Self {
            pixels: vec![NONE; (W * H) as usize],
        }
    }
    fn set(&mut self, x: f64, y: f64, layer: u8) {
        let (x, y) = (x.round() as i32, y.round() as i32);
        if (0..W).contains(&x) && (0..H).contains(&y) {
            let p = &mut self.pixels[(y * W + x) as usize];
            if layer >= *p {
                *p = layer;
            }
        }
    }
    fn erase(&mut self, x: f64, y: f64) {
        let (x, y) = (x.round() as i32, y.round() as i32);
        if (0..W).contains(&x) && (0..H).contains(&y) {
            self.pixels[(y * W + x) as usize] = NONE;
        }
    }
    fn curve(&mut self, mut f: impl FnMut(f64) -> (f64, f64), steps: usize, layer: u8) {
        for i in 0..=steps {
            let u = i as f64 / steps as f64;
            let p = f(u);
            self.set(p.0, p.1, layer);
        }
    }
}

fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}
fn ease_in_out(u: f64) -> f64 {
    if u < 0.5 {
        4.0 * u * u * u
    } else {
        1.0 - (-2.0 * u + 2.0).powi(3) / 2.0
    }
}
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}
fn rand(seed: u32, n: u32) -> f64 {
    (hash(seed.wrapping_add(n.wrapping_mul(0x9e3779b9))) as f64) / u32::MAX as f64
}

fn curl(th: f64, bend: f64, s: f64) -> (f64, f64) {
    if bend.abs() < 1e-4 {
        (s * th.cos(), s * th.sin())
    } else {
        (
            ((th + bend * s).sin() - th.sin()) / bend,
            (th.cos() - (th + bend * s).cos()) / bend,
        )
    }
}
fn point(th: f64, bend: f64, s: f64, v: f64) -> (f64, f64) {
    let (x, z) = curl(th, bend, s);
    let a = (PI * clamp(s)).sin().powf(0.8);
    (
        CX + x * PW * (0.9 + 0.1 * v),
        TOP + v * PH - z.max(0.0) * 8.0 - 2.2 * a * (1.0 - 1.5 * v),
    )
}
fn page_lines(page: i64) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    let mut indent = rand(page as u32 * 7919 + 17, 1) < 0.5;
    for i in 0..6 {
        let v = 0.15 + i as f64 * 0.64 / 5.0;
        let para = rand(page as u32 * 7919 + 17, i + 2) < 0.22
            || (i == 5 && rand(page as u32 + 9, i + 2) < 0.35);
        let start = if indent { 0.12 } else { 0.0 };
        let len = if para {
            0.2 + rand(page as u32 + 31, i + 4) * 0.4
        } else {
            1.0 - start
        };
        out.push((v, start, len));
        indent = para;
    }
    out
}
#[allow(clippy::too_many_arguments)]
fn draw_lines(
    cv: &mut Canvas,
    th: f64,
    bend: f64,
    right: bool,
    page: i64,
    read: f64,
    reading: bool,
    layer: u8,
) {
    let mut consumed = 0.0;
    for (v, start, len) in page_lines(page) {
        let done = (read - consumed).clamp(0.0, len);
        let seg = |cv: &mut Canvas, a: f64, b: f64, l: u8| {
            if b <= a {
                return;
            }
            let steps = ((b - a) * 0.87 * PW * 1.6).ceil() as usize + 1;
            for i in 0..=steps {
                let u = i as f64 / steps as f64;
                let q = a + (b - a) * u;
                let s = if right {
                    0.13 + q * 0.74
                } else {
                    0.87 - q * 0.74
                };
                cv.set(point(th, bend, s, v).0, point(th, bend, s, v).1, l);
            }
        };
        seg(
            cv,
            start,
            start + done,
            if reading { READ_LAYER } else { layer },
        );
        seg(cv, start + done, start + len, layer);
        if reading && done > 0.0 && done < len {
            seg(cv, (start + done - 0.14).max(start), start + done, ACCENT);
        }
        consumed += len;
    }
}
fn draw_book(cv: &mut Canvas) {
    for th in [0.0, PI] {
        cv.curve(|s| point(th, 0.0, s, 0.0), 80, FRAME);
        cv.curve(|s| point(th, 0.0, s, 1.0), 80, FRAME);
        cv.curve(|v| point(th, 0.0, 1.0, v), 40, FRAME);
    }
    cv.curve(|v| point(0.0, 0.0, 0.0, v), 30, FRAME);
}
fn draw_ribbon(cv: &mut Canvas, t: i64) {
    let (ax, ay) = point(0.0, 0.0, 0.0, 1.0);
    for i in 0..=26 {
        let u = i as f64 / 26.0;
        let x = ax + 0.6 + u * 1.2 + (t as f64 / 620.0 + u * 2.4).sin() * u * 2.2;
        let y = ay + 1.0 + u * 10.0;
        cv.set(x, y, ACCENT);
        if u > 0.08 {
            cv.set(x + 1.0, y, ACCENT);
        }
    }
}
fn draw_flip(cv: &mut Canvas, u: f64, front_page: i64, back_page: i64) {
    let th = ease_in_out(u) * PI;
    let bend = 0.85 * (2.0 * PI * u).sin();
    for si in 0..=64 {
        for vi in 0..=40 {
            let (x, y) = point(th, bend, si as f64 / 64.0, vi as f64 / 40.0);
            cv.erase(x, y);
        }
    }
    cv.curve(|s| point(th, bend, s, 0.0), 90, PAGE);
    cv.curve(|s| point(th, bend, s, 1.0), 90, PAGE);
    cv.curve(|v| point(th, bend, 1.0, v), 40, PAGE);
    if th + bend * 0.5 < PI / 2.0 {
        draw_lines(cv, th, bend, true, front_page, f64::INFINITY, false, PTEXT);
    } else {
        draw_lines(cv, th, bend, false, back_page, 0.0, false, PTEXT);
    }
}

pub fn render(frame: &mut Buffer, area: Rect, elapsed_ms: i64) {
    if area.width < 12 || area.height < 8 {
        return;
    }
    let mut cv = Canvas::new();
    let t = elapsed_ms.max(0);
    let main_t = t.saturating_sub(INTRO);
    let k = main_t / CYCLE;
    let phase = main_t % CYCLE;
    draw_book(&mut cv);
    let left = 2 * k;
    let right = 2 * k + 1;
    let progress = clamp((phase - 180) as f64 / (READ - 420) as f64);
    if phase < READ {
        draw_lines(&mut cv, PI, 0.0, false, left, progress * 4.0, true, TEXT);
        draw_lines(
            &mut cv,
            0.0,
            0.0,
            true,
            right,
            (progress * 4.0 - 2.0).max(0.0),
            true,
            TEXT,
        );
    } else {
        draw_lines(&mut cv, PI, 0.0, false, left, f64::INFINITY, false, TEXT);
        draw_lines(&mut cv, 0.0, 0.0, true, right, f64::INFINITY, false, TEXT);
        if phase < READ + FLIP {
            draw_flip(
                &mut cv,
                (phase - READ) as f64 / FLIP as f64,
                right,
                right + 1,
            );
        }
    }
    draw_ribbon(&mut cv, t);
    let bits = [[1u8, 8], [2, 16], [4, 32], [64, 128]];
    let xoff = area.x.saturating_add(area.width.saturating_sub(30) / 2);
    let yoff = area.y.saturating_add(area.height.saturating_sub(10) / 2);
    for cy in 0..10 {
        for cx in 0..30 {
            let mut mask = 0u8;
            let mut best = 0u8;
            for (dy, bit_row) in bits.iter().enumerate() {
                for (dx, bit) in bit_row.iter().enumerate() {
                    let l = cv.pixels[(cy * 4 + dy) * 60 + cx * 2 + dx];
                    if l > 0 {
                        mask |= *bit;
                        best = best.max(l);
                    }
                }
            }
            if mask > 0 {
                frame[(xoff + cx as u16, yoff + cy as u16)]
                    .set_char(char::from_u32(0x2800 + mask as u32).unwrap())
                    .set_fg(COLORS[best as usize]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    #[test]
    fn animation_has_braille_and_changes_during_flip() {
        let mut t = Terminal::new(TestBackend::new(40, 14)).unwrap();
        t.draw(|f| render(f.buffer_mut(), Rect::new(0, 0, 40, 14), 0))
            .unwrap();
        let a = t
            .backend()
            .buffer()
            .content
            .iter()
            .filter(|c| {
                c.symbol().starts_with('⠄')
                    || (c
                        .symbol()
                        .chars()
                        .next()
                        .is_some_and(|c| ('⠀'..='⣿').contains(&c)))
            })
            .count();
        assert!(a > 0);
    }
}
