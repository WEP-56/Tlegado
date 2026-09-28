// ─────────────────────────────────────────────────────────────
//  Tlegado 首页 logo 动画 —— 盲文点阵（Braille）渲染器
//
//  纯函数、零依赖：renderFrame(t) → 行/片段（字符 + 图层）
//  每个终端字符 = 2×4 个子像素（U+2800 ~ U+28FF），
//  画布 30×10 字符 = 60×40 子像素。
//
//  移植 ratatui：
//    · 同一套算法写入 Vec<u8> 像素缓冲
//    · 自定义 Widget 在 render() 中按 2×4 打包成 braille 字符，
//      buf[(x, y)].set_char(ch).set_fg(PALETTE[layer])
//    · 或直接使用 Canvas + Marker::Braille（颜色同样按单元格取最后/最高层）
//    · 30fps tick 驱动，t = 启动后毫秒数
// ─────────────────────────────────────────────────────────────

export const COLS = 30;
export const ROWS = 10;
const W = COLS * 2;
const H = ROWS * 4;

// 书本几何（单位：子像素）
const CX = W / 2; // 书脊 x
const TOP = 8; // 页面顶边 y
const PH = 18; // 页面高度
const PW = 26; // 单页宽度
const LIFT = 8; // 翻起高度 → 屏幕上移量
const ARCH = 2.2; // 书页鼓起弧度
const NL = 6; // 每页文字行数
const M = 0.13; // 页边距（占页宽比例）
const CW = 1 - 2 * M; // 文字栏宽

// 时间轴（ms）
const READ = 2600;
const FLIP = 1150;
const REST = 350;
const CYCLE = READ + FLIP + REST;
export const INTRO = 1800;

// 图层 = 优先级（同一字符格内取最高者的颜色）
export const L = {
  NONE: 0,
  TEXT: 1, // 未读文字
  DIM: 2, // 余烬 / 开场散点
  FRAME: 3, // 书本轮廓
  READ: 4, // 已读文字
  PTEXT: 5, // 翻动页上的文字
  PAGE: 6, // 翻动页轮廓
  ACCENT: 7, // 阅读光标 / 丝带 / 火花
} as const;

export const PALETTE: Record<number, string> = {
  0: "transparent",
  1: "#4b4b4b",
  2: "#8a6232",
  3: "#7e7e7e",
  4: "#b8b8b8",
  5: "#c9c9c9",
  6: "#efefef",
  7: "#e3a35a",
};

type Pt = [number, number];

// ── 工具 ────────────────────────────────────────────────────
function rng(seed: number) {
  let a = seed >>> 0 || 1;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
function hash2(a: number, b: number) {
  let h = Math.imul(a ^ 0x9e3779b9, 0x85ebca6b) ^ Math.imul(b + 0x632be5ab, 0xc2b2ae35);
  h ^= h >>> 13;
  h = Math.imul(h, 0x27d4eb2f);
  return (h ^ (h >>> 16)) >>> 0;
}
const clamp01 = (x: number) => Math.max(0, Math.min(1, x));
const easeInOut = (u: number) => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2);
const easeOut = (u: number) => 1 - Math.pow(1 - u, 3);

class Canvas {
  buf = new Uint8Array(W * H);
  set(x: number, y: number, l: number) {
    const xi = Math.round(x);
    const yi = Math.round(y);
    if (xi < 0 || yi < 0 || xi >= W || yi >= H) return;
    this.buf[yi * W + xi] = l;
  }
  curve(f: (u: number) => Pt, steps: number, l: number) {
    for (let i = 0; i <= steps; i++) {
      const [x, y] = f(i / steps);
      this.set(x, y, l);
    }
  }
  line(a: Pt, b: Pt, l: number) {
    const n = Math.ceil(Math.max(Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1])) * 1.5) + 1;
    for (let i = 0; i <= n; i++) this.set(a[0] + ((b[0] - a[0]) * i) / n, a[1] + ((b[1] - a[1]) * i) / n, l);
  }
}

// ── 书页几何 ────────────────────────────────────────────────
/**
 * 书页横截面：从书脊出发、弧长 s∈[0,1] 的一条弯曲曲线。
 * th = 书脊处角度（0 平放右侧，π 平放左侧），b = 弯曲率（外缘相对书脊的角度差）
 * 返回 (x, z)：x 为水平（右正），z 为抬起高度。
 */
function curl(th: number, b: number, s: number): Pt {
  if (Math.abs(b) < 1e-4) return [s * Math.cos(th), s * Math.sin(th)];
  return [(Math.sin(th + b * s) - Math.sin(th)) / b, (Math.cos(th) - Math.cos(th + b * s)) / b];
}

/** 书页上一点 (s: 书脊→外缘, v: 顶→底) 投影到屏幕 */
function P(th: number, b: number, s: number, v: number): Pt {
  const [x, z0] = curl(th, b, s);
  const z = Math.max(0, z0);
  const a = Math.pow(Math.sin(Math.PI * clamp01(s)), 0.8);
  const persp = 0.9 + 0.1 * v; // 近大远小
  return [CX + x * PW * persp, TOP + v * PH - z * LIFT - ARCH * a * (1 - 1.5 * v)];
}

// ── 页面内容（按页码确定性生成） ──────────────────────────
interface Line {
  v: number;
  start: number;
  len: number;
}
const lineCache = new Map<number, Line[]>();
function pageLines(p: number): Line[] {
  const hit = lineCache.get(p);
  if (hit) return hit;
  const r = rng(p * 7919 + 17);
  const out: Line[] = [];
  let indent = r() < 0.5;
  for (let i = 0; i < NL; i++) {
    const v = 0.15 + (i * 0.64) / (NL - 1);
    const endPara = r() < 0.22 || (i === NL - 1 && r() < 0.35);
    const start = indent ? 0.12 : 0;
    const len = endPara ? 0.2 + r() * 0.4 : 1 - start;
    out.push({ v, start, len });
    indent = endPara;
  }
  lineCache.set(p, out);
  return out;
}
const sumLen = (ls: Line[]) => ls.reduce((s, l) => s + l.len, 0);

/**
 * 绘制一页文字。right=true 为右页排版（从书脊起笔），否则左页排版（从外缘起笔）。
 * read = 本页已读长度（累计 len 单位）
 */
function drawLines(
  cv: Canvas, th: number, b: number, right: boolean, lines: Line[],
  read: number, head: boolean, readLayer: number = L.READ, unreadLayer: number = L.TEXT,
) {
  const colS = (a: number) => (right ? M + a * CW : 1 - M - a * CW);
  const seg = (v: number, a0: number, a1: number, l: number) => {
    if (a1 <= a0) return;
    const steps = Math.ceil((a1 - a0) * CW * PW * 1.6) + 1;
    for (let i = 0; i <= steps; i++) {
      const [x, y] = P(th, b, colS(a0 + ((a1 - a0) * i) / steps), v);
      cv.set(x, y, l);
    }
  };
  let c = 0;
  for (const ln of lines) {
    const pr = Math.max(0, Math.min(ln.len, read - c));
    seg(ln.v, ln.start, ln.start + pr, readLayer);
    seg(ln.v, ln.start + pr, ln.start + ln.len, unreadLayer);
    if (head && pr > 0 && pr < ln.len) seg(ln.v, Math.max(ln.start, ln.start + pr - 0.14), ln.start + pr, L.ACCENT);
    c += ln.len;
  }
}

// ── 场景 ────────────────────────────────────────────────────
function drawBook(cv: Canvas) {
  for (const th of [0, Math.PI]) {
    cv.curve((s) => P(th, 0, s, 0), 80, L.FRAME);
    cv.curve((s) => P(th, 0, s, 1), 80, L.FRAME);
    cv.curve((v) => P(th, 0, 1, v), 40, L.FRAME);
    // 书页厚度
    for (const k of [1, 2]) {
      const off = 0.4 + k * 2;
      const sc = 1 + 0.014 * k;
      const g = (s: number): Pt => {
        const [x, y] = P(th, 0, s, 1);
        return [CX + (x - CX) * sc, y + off];
      };
      cv.curve((u) => g(0.03 + u * 0.97), 80, k === 1 ? L.FRAME : L.TEXT);
      cv.line(P(th, 0, 1, 1), g(1), L.FRAME);
    }
  }
  cv.curve((v) => P(0, 0, 0, v), 30, L.FRAME); // 书脊
}

function drawRibbon(cv: Canvas, t: number) {
  const [ax, ay] = P(0, 0, 0, 1);
  let end: Pt = [ax, ay];
  for (let i = 0; i <= 26; i++) {
    const u = i / 26;
    const x = ax + 0.6 + u * 1.2 + Math.sin(t / 620 + u * 2.4) * u * 2.2;
    const y = ay + 1 + u * 10;
    cv.set(x, y, L.ACCENT);
    if (u > 0.08) cv.set(x + 1, y, L.ACCENT);
    end = [x, y];
  }
  // 燕尾
  cv.set(end[0] - 0.4, end[1] + 1.2, L.ACCENT);
  cv.set(end[0] + 1.6, end[1] + 1.2, L.ACCENT);
}

const flipParams = (u: number) => ({ th: easeInOut(u) * Math.PI, b: 0.85 * Math.sin(2 * Math.PI * u) });

function drawFlip(cv: Canvas, u: number, frontP: number, backP: number) {
  const { th, b } = flipParams(u);
  // 遮挡：擦除翻动页覆盖区域
  for (let si = 0; si <= 64; si++)
    for (let vi = 0; vi <= 40; vi++) {
      const [x, y] = P(th, b, si / 64, vi / 40);
      cv.set(x, y, L.NONE);
    }
  const edge = u > 0.03 && u < 0.95 ? L.PAGE : L.FRAME;
  cv.curve((s) => P(th, b, s, 0), 90, edge);
  cv.curve((s) => P(th, b, s, 1), 90, edge);
  cv.curve((v) => P(th, b, 1, v), 40, edge);
  const front = th + b * 0.5 < Math.PI / 2;
  if (front) drawLines(cv, th, b, true, pageLines(frontP), Infinity, false, L.PTEXT);
  else drawLines(cv, th, b, false, pageLines(backP), 0, false);
}

function drawSparks(cv: Canvas, t: number, k: number) {
  const EMIT = 0.42;
  for (const kk of [k, k - 1]) {
    if (kk < 0) continue;
    const tau = t - (kk * CYCLE + READ + EMIT * FLIP);
    if (tau < 0 || tau > 1400) continue;
    const r = rng(kk * 131 + 7);
    const { th, b } = flipParams(EMIT);
    for (let i = 0; i < 9; i++) {
      const s = 0.55 + r() * 0.45;
      const v = r() * 0.5;
      const vx = (r() - 0.5) * 0.03;
      const vy = -(0.008 + r() * 0.016);
      const life = 600 + r() * 700;
      const ph = r() * 6;
      if (tau > life) continue;
      if (hash2(kk * 16 + i, Math.floor(tau / 70)) % 5 === 0) continue; // 闪烁
      const [ox, oy] = P(th, b, s, v);
      cv.set(ox + vx * tau + Math.sin(tau / 140 + ph) * 0.9, oy + vy * tau, tau < life * 0.5 ? L.ACCENT : L.DIM);
    }
  }
}

/** 主循环：读左页 → 读右页 → 翻页 → 停顿 */
function drawMain(cv: Canvas, t: number): number {
  const k = Math.floor(t / CYCLE);
  const ph = t - k * CYCLE;
  const reading = ph < READ;
  const flipping = !reading && ph < READ + FLIP;
  const leftP = ph >= READ + FLIP ? 2 * k + 2 : 2 * k;
  const rightP = reading ? 2 * k + 1 : 2 * k + 3;

  drawBook(cv);
  const Lp = pageLines(leftP);
  const Rp = pageLines(rightP);
  let rl = 0;
  let rr = 0;
  if (reading) {
    const q = clamp01((ph - 180) / (READ - 420));
    const sl = sumLen(Lp);
    const rt = q * (sl + sumLen(Rp));
    rl = Math.min(rt, sl);
    rr = Math.max(0, rt - sl);
  } else if (flipping) rl = Infinity;
  drawLines(cv, Math.PI, 0, false, Lp, rl, reading);
  drawLines(cv, 0, 0, true, Rp, rr, reading);
  drawRibbon(cv, t);
  if (flipping) drawFlip(cv, (ph - READ) / FLIP, 2 * k + 1, 2 * k + 2);
  drawSparks(cv, t, k);
  return ph < READ + FLIP / 2 ? k : k + 1;
}

// ── 开场：散点汇聚 ─────────────────────────────────────────
interface Dot { x: number; y: number; l: number; ox: number; oy: number; d: number; dur: number }
let introDots: Dot[] | null = null;
function drawIntro(cv: Canvas, t: number) {
  if (!introDots) {
    const tc = new Canvas();
    drawMain(tc, 0);
    const r = rng(2024);
    introDots = [];
    for (let y = 0; y < H; y++)
      for (let x = 0; x < W; x++) {
        const l = tc.buf[y * W + x];
        if (l) introDots.push({ x, y, l, ox: r() * W, oy: r() * H, d: (x / W) * 450 + r() * 300, dur: 650 + r() * 350 });
      }
  }
  for (const p of introDots) {
    const q = clamp01((t - p.d) / p.dur);
    if (q <= 0) {
      if (hash2(p.x * 97 + p.y, Math.floor(t / 90)) % 3 === 0) cv.set(p.ox, p.oy, L.DIM);
      continue;
    }
    const e = easeOut(q);
    cv.set(p.ox + (p.x - p.ox) * e, p.oy + (p.y - p.oy) * e, e > 0.7 ? p.l : L.DIM);
  }
}

// ── 输出 ────────────────────────────────────────────────────
export interface Span { text: string; layer: number }
export interface Frame { rows: Span[][]; spread: number }

const BITS = [
  [0x01, 0x08],
  [0x02, 0x10],
  [0x04, 0x20],
  [0x40, 0x80],
];

function pack(buf: Uint8Array): Span[][] {
  const rows: Span[][] = [];
  for (let cy = 0; cy < ROWS; cy++) {
    const row: Span[] = [];
    for (let cx = 0; cx < COLS; cx++) {
      let bits = 0;
      let best = 0;
      for (let dy = 0; dy < 4; dy++)
        for (let dx = 0; dx < 2; dx++) {
          const l = buf[(cy * 4 + dy) * W + cx * 2 + dx];
          if (l) {
            bits |= BITS[dy][dx];
            if (l > best) best = l;
          }
        }
      const ch = String.fromCharCode(0x2800 + bits);
      const last = row[row.length - 1];
      if (last && last.layer === best) last.text += ch;
      else row.push({ text: ch, layer: best });
    }
    rows.push(row);
  }
  return rows;
}

/** t：动画启动后的毫秒数 */
export function renderFrame(t: number): Frame {
  const cv = new Canvas();
  let spread = 0;
  if (t < INTRO) drawIntro(cv, t);
  else spread = drawMain(cv, t - INTRO);
  return { rows: pack(cv.buf), spread };
}

/** 调试：返回原始像素缓冲 */
export function renderPixels(t: number): { w: number; h: number; buf: Uint8Array } {
  const cv = new Canvas();
  if (t < INTRO) drawIntro(cv, t);
  else drawMain(cv, t - INTRO);
  return { w: W, h: H, buf: cv.buf };
}

/** 点击“立即翻页”：返回需要叠加的时间偏移 */
export function flipDelta(t: number): number {
  if (t < INTRO) return 0;
  const ph = (t - INTRO) % CYCLE;
  return ph < READ ? READ - ph : 0;
}
