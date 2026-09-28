// ─────────────────────────────────────────────────────────────
//  legado-tui 模拟数据
//  字段命名尽量贴近 legado (阅读3.0) 的数据结构，方便后续对接
// ─────────────────────────────────────────────────────────────

export type BookKind = "local" | "network";

export interface Book {
  id: string;
  title: string;
  author: string;
  kind: BookKind;
  /** 本地书格式 */
  format?: "TXT" | "EPUB" | "UMD" | "PDF";
  /** 本地路径 */
  path?: string;
  /** 网络书的来源书源名 */
  origin?: string;
  group: string;
  category: string;
  status: "连载" | "完结";
  words: string;
  total: number;
  /** 已读到的章节 index（0 基） */
  read: number;
  latest: string;
  lastRead: string;
  newCount: number;
  intro: string;
}

export interface BookSource {
  id: string;
  bookSourceName: string;
  bookSourceGroup: string;
  bookSourceUrl: string;
  enabled: boolean;
  enabledExplore: boolean;
  respondTime: number; // ms, -1 = 超时
  lastUpdate: string;
  explore: string[];
  searchUrl: string;
}

export interface PurifyRule {
  id: string;
  name: string;
  pattern: string;
  replacement: string;
  isRegex: boolean;
  scope: string;
  enabled: boolean;
}

export interface HistoryItem {
  id: string;
  bookId: string;
  title: string;
  author: string;
  chapter: string;
  chapterIndex: number;
  time: string;
  duration: string;
  origin: string;
}

// ── 章节名 ──────────────────────────────────────────────────
const CH_NAMES = [
  "雾中来客", "旧城钟声", "灯下残卷", "北境之风", "无名之碑", "长夜将尽", "渡口", "第二封信",
  "裂隙", "星图", "归途", "空城", "青铜门", "雨夜", "晨钟", "逆旅", "余烬", "回响",
  "潮汐", "少年游", "山海之间", "镜中人", "白塔", "秋水", "暗河", "问剑", "长街", "故人",
];

export const chapterTitle = (i: number) => `第${i + 1}章 ${CH_NAMES[(i * 7) % CH_NAMES.length]}`;

// ── 书架 ────────────────────────────────────────────────────
export const SHELF: Book[] = [
  {
    id: "b1", title: "诡秘之主", author: "爱潜水的乌贼", kind: "network", origin: "起点中文网",
    group: "完结", category: "玄幻", status: "完结", words: "446.5万", total: 1432, read: 812,
    latest: "第1432章 新的征程", lastRead: "10分钟前", newCount: 0,
    intro: "蒸汽与机械的浪潮中，谁能触及非凡？历史和黑暗的迷雾里，又是谁在耳语？我从诡秘中醒来，睁眼看见这个世界……",
  },
  {
    id: "b2", title: "道诡异仙", author: "狐尾的笔", kind: "network", origin: "起点中文网",
    group: "追更", category: "仙侠", status: "连载", words: "312.8万", total: 1068, read: 1062,
    latest: "第1068章 心素", lastRead: "2小时前", newCount: 6,
    intro: "诡异的天道，异常的仙佛，是真？是假？陷入迷惘的李火旺无法分辨。",
  },
  {
    id: "b3", title: "大奉打更人", author: "卖报小郎君", kind: "network", origin: "番茄小说",
    group: "完结", category: "仙侠", status: "完结", words: "380.2万", total: 1284, read: 355,
    latest: "第1284章 大结局", lastRead: "昨天 22:14", newCount: 0,
    intro: "这个世界，有儒；有道；有佛；有妖；有术士。警校毕业的许七安幽幽醒来，发现自己身处牢狱之中。",
  },
  {
    id: "b4", title: "深空彼岸", author: "辰东", kind: "network", origin: "笔趣阁①",
    group: "追更", category: "科幻", status: "连载", words: "402.1万", total: 1520, read: 1498,
    latest: "第1520章 彼岸花开", lastRead: "3天前", newCount: 22,
    intro: "浩瀚的宇宙中，一片星系的生灭，也不过是刹那的斑驳流光。仰望星空，总有种结局已注定的伤感。",
  },
  {
    id: "b5", title: "凡人修仙传", author: "忘语", kind: "network", origin: "纵横中文网",
    group: "经典", category: "仙侠", status: "完结", words: "771.6万", total: 2446, read: 2446,
    latest: "第2446章 飞升仙界", lastRead: "上周", newCount: 0,
    intro: "一个普通山村小子，偶然下进入到当地江湖小门派，成了一名记名弟子。",
  },
  {
    id: "b6", title: "我在精神病院学斩神", author: "三九音域", kind: "network", origin: "番茄小说",
    group: "追更", category: "都市", status: "连载", words: "298.4万", total: 1356, read: 901,
    latest: "第1356章 天庭", lastRead: "5天前", newCount: 3,
    intro: "你是否想过，在霓虹璀璨的都市之下，潜藏着来自古老神话的怪物？",
  },
  {
    id: "b7", title: "三体", author: "刘慈欣", kind: "local", format: "EPUB", path: "~/Books/三体全集.epub",
    group: "经典", category: "科幻", status: "完结", words: "88.0万", total: 104, read: 37,
    latest: "第104章 尾声", lastRead: "1小时前", newCount: 0,
    intro: "文化大革命如火如荼进行的同时，军方探寻外星文明的绝秘计划“红岸工程”取得了突破性进展。",
  },
  {
    id: "b8", title: "活着", author: "余华", kind: "local", format: "TXT", path: "~/Books/活着.txt",
    group: "经典", category: "文学", status: "完结", words: "12.1万", total: 12, read: 12,
    latest: "第12章 老人与牛", lastRead: "上个月", newCount: 0,
    intro: "讲述了一个人历尽世间沧桑和磨难的一生，亦将中国大半个世纪的社会变迁凝缩其间。",
  },
  {
    id: "b9", title: "百年孤独", author: "加西亚·马尔克斯", kind: "local", format: "EPUB", path: "~/Books/百年孤独.epub",
    group: "经典", category: "文学", status: "完结", words: "25.3万", total: 20, read: 4,
    latest: "第20章", lastRead: "2周前", newCount: 0,
    intro: "多年以后，面对行刑队，奥雷里亚诺·布恩迪亚上校将会回想起父亲带他去见识冰块的那个遥远的下午。",
  },
  {
    id: "b10", title: "围城", author: "钱锺书", kind: "local", format: "TXT", path: "~/Books/围城.txt",
    group: "经典", category: "文学", status: "完结", words: "23.0万", total: 9, read: 0,
    latest: "第9章", lastRead: "未读", newCount: 0,
    intro: "城外的人想冲进去，城里的人想逃出来。",
  },
];

// ── 书源 ────────────────────────────────────────────────────
export const SOURCES: BookSource[] = [
  {
    id: "s1", bookSourceName: "起点中文网", bookSourceGroup: "正版", bookSourceUrl: "https://www.qidian.com",
    enabled: true, enabledExplore: true, respondTime: 212, lastUpdate: "2026-01-12",
    explore: ["玄幻", "奇幻", "仙侠", "都市", "历史", "科幻", "悬疑"],
    searchUrl: "/soushu/{{key}}.html",
  },
  {
    id: "s2", bookSourceName: "番茄小说", bookSourceGroup: "正版", bookSourceUrl: "https://fanqienovel.com",
    enabled: true, enabledExplore: true, respondTime: 188, lastUpdate: "2026-02-03",
    explore: ["男频热榜", "女频热榜", "新书榜", "完结榜", "巅峰榜"],
    searchUrl: "/api/search?query={{key}}",
  },
  {
    id: "s3", bookSourceName: "笔趣阁①", bookSourceGroup: "聚合", bookSourceUrl: "https://www.biquge.example",
    enabled: true, enabledExplore: true, respondTime: 540, lastUpdate: "2025-11-20",
    explore: ["玄幻魔法", "武侠修真", "都市言情", "历史军事", "网游竞技", "排行榜"],
    searchUrl: "/search.php?q={{key}}",
  },
  {
    id: "s4", bookSourceName: "纵横中文网", bookSourceGroup: "正版", bookSourceUrl: "https://www.zongheng.com",
    enabled: true, enabledExplore: true, respondTime: 301, lastUpdate: "2025-12-08",
    explore: ["月票榜", "畅销榜", "新书榜", "完本"],
    searchUrl: "/search?keyword={{key}}",
  },
  {
    id: "s5", bookSourceName: "晋江文学城", bookSourceGroup: "正版", bookSourceUrl: "https://www.jjwxc.net",
    enabled: true, enabledExplore: true, respondTime: 422, lastUpdate: "2025-10-30",
    explore: ["言情", "纯爱", "衍生", "无CP", "完结金榜"],
    searchUrl: "/search.php?kw={{key}}",
  },
  {
    id: "s6", bookSourceName: "七猫小说", bookSourceGroup: "正版", bookSourceUrl: "https://www.qimao.com",
    enabled: true, enabledExplore: false, respondTime: 260, lastUpdate: "2026-01-28",
    explore: [], searchUrl: "/search/{{key}}",
  },
  {
    id: "s7", bookSourceName: "69书吧", bookSourceGroup: "聚合", bookSourceUrl: "https://www.69shu.example",
    enabled: false, enabledExplore: true, respondTime: 690, lastUpdate: "2025-08-14",
    explore: ["全部", "排行"], searchUrl: "/modules/article/search.php?searchkey={{key}}",
  },
  {
    id: "s8", bookSourceName: "书海阁", bookSourceGroup: "聚合", bookSourceUrl: "https://www.shuhai.example",
    enabled: false, enabledExplore: false, respondTime: -1, lastUpdate: "2025-05-02",
    explore: [], searchUrl: "/s?q={{key}}",
  },
];

// ── 发现书库（按 书源+分类 生成） ──────────────────────────
const POOL: [string, string, string][] = [
  ["剑来", "烽火戏诸侯", "仙侠"], ["雪中悍刀行", "烽火戏诸侯", "武侠"], ["庆余年", "猫腻", "历史"],
  ["将夜", "猫腻", "玄幻"], ["赤心巡天", "情何以甚", "仙侠"], ["灵境行者", "卖报小郎君", "都市"],
  ["宿命之环", "爱潜水的乌贼", "奇幻"], ["夜的命名术", "会说话的肘子", "都市"], ["十日终焉", "杀虫队队员", "悬疑"],
  ["光阴之外", "耳根", "仙侠"], ["神秘复苏", "佛前献花", "悬疑"], ["学霸的黑科技系统", "晨星LL", "科幻"],
  ["临高启明", "吹牛者", "历史"], ["明朝那些事儿", "当年明月", "历史"], ["天官赐福", "墨香铜臭", "纯爱"],
  ["全职高手", "蝴蝶蓝", "网游"], ["斗罗大陆", "唐家三少", "玄幻"], ["遮天", "辰东", "玄幻"],
  ["一世之尊", "爱潜水的乌贼", "仙侠"], ["择天记", "猫腻", "玄幻"], ["长安的荔枝", "马伯庸", "历史"],
  ["我的治愈系游戏", "我会修空调", "悬疑"], ["修真聊天群", "圣骑士的传说", "都市"], ["大王饶命", "会说话的肘子", "都市"],
  ["星门", "老鹰吃小鸡", "都市"], ["黎明之剑", "远瞳", "奇幻"], ["异常生物见闻录", "远瞳", "科幻"],
  ["琅琊榜", "海宴", "历史"], ["撒野", "巫哲", "纯爱"], ["小兵传奇", "玄雨", "科幻"],
];

const hash = (s: string) => {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return Math.abs(h);
};

const discoverCache = new Map<string, Book[]>();

export function getDiscoverBooks(source: BookSource, cat: string): Book[] {
  const key = source.id + "|" + cat;
  const hit = discoverCache.get(key);
  if (hit) return hit;
  const seed = hash(key);
  const n = 9 + (seed % 5);
  const list: Book[] = [];
  for (let i = 0; i < n; i++) {
    const [title, author, category] = POOL[(seed + i * 7) % POOL.length];
    if (list.some((b) => b.title === title)) continue;
    const h = hash(title + key);
    const total = 300 + (h % 2200);
    const done = h % 3 === 0;
    list.push({
      id: `d-${source.id}-${hash(title)}`,
      title, author, kind: "network", origin: source.bookSourceName,
      group: "未分组", category, status: done ? "完结" : "连载",
      words: `${(total * 0.31).toFixed(1)}万`, total, read: 0,
      latest: chapterTitle(total - 1), lastRead: "未读", newCount: 0,
      intro: `【${cat}】${title}，${author}著。${done ? "已完结，可放心入坑。" : "连载中，更新稳定。"}${
        ["少年负剑出乡关，一路向北。", "迷雾之后，是另一个世界的规则。", "他本想平凡一生，命运却另有安排。", "长夜漫漫，唯有灯火与书卷相伴。"][h % 4]
      }`,
    });
  }
  discoverCache.set(key, list);
  return list;
}

/** 模拟跨书源搜索 */
export function searchAll(q: string, sources: BookSource[]): { source: BookSource; books: Book[] }[] {
  const kw = q.trim();
  return sources
    .filter((s) => s.enabled)
    .map((s) => {
      const books: Book[] = [];
      const seen = new Set<string>();
      [...SHELF.filter((b) => b.kind === "network"), ...POOL.map(([t, a, c]) => ({ title: t, author: a, category: c }) as Book)]
        .filter((b) => b.title.includes(kw) || b.author.includes(kw))
        .forEach((b) => {
          if (seen.has(b.title) || hash(b.title + s.id) % 4 === 0) return;
          seen.add(b.title);
          const total = 300 + (hash(b.title) % 1800);
          books.push({
            ...b,
            id: `q-${s.id}-${hash(b.title)}`,
            kind: "network", origin: s.bookSourceName, group: "未分组",
            status: b.status ?? "连载", words: b.words ?? `${(total * 0.3).toFixed(1)}万`,
            total: b.total ?? total, read: 0, latest: b.latest ?? chapterTitle(total - 1),
            lastRead: "未读", newCount: 0, intro: b.intro ?? `${b.title}，${b.author}著。`,
          });
        });
      return { source: s, books };
    });
}

// ── 历史 ────────────────────────────────────────────────────
export const HISTORY: HistoryItem[] = [
  { id: "h1", bookId: "b1", title: "诡秘之主", author: "爱潜水的乌贼", chapter: chapterTitle(812), chapterIndex: 812, time: "今天 21:42", duration: "48分钟", origin: "起点中文网" },
  { id: "h2", bookId: "b7", title: "三体", author: "刘慈欣", chapter: chapterTitle(37), chapterIndex: 37, time: "今天 20:31", duration: "1小时12分", origin: "本地 EPUB" },
  { id: "h3", bookId: "b2", title: "道诡异仙", author: "狐尾的笔", chapter: chapterTitle(1062), chapterIndex: 1062, time: "今天 19:05", duration: "22分钟", origin: "起点中文网" },
  { id: "h4", bookId: "b3", title: "大奉打更人", author: "卖报小郎君", chapter: chapterTitle(355), chapterIndex: 355, time: "昨天 22:14", duration: "35分钟", origin: "番茄小说" },
  { id: "h5", bookId: "b4", title: "深空彼岸", author: "辰东", chapter: chapterTitle(1498), chapterIndex: 1498, time: "03-14 23:50", duration: "2小时03分", origin: "笔趣阁①" },
  { id: "h6", bookId: "b6", title: "我在精神病院学斩神", author: "三九音域", chapter: chapterTitle(901), chapterIndex: 901, time: "03-12 12:20", duration: "15分钟", origin: "番茄小说" },
  { id: "h7", bookId: "b9", title: "百年孤独", author: "加西亚·马尔克斯", chapter: chapterTitle(4), chapterIndex: 4, time: "03-02 09:40", duration: "41分钟", origin: "本地 EPUB" },
  { id: "h8", bookId: "b5", title: "凡人修仙传", author: "忘语", chapter: chapterTitle(2445), chapterIndex: 2445, time: "02-26 01:13", duration: "3小时20分", origin: "纵横中文网" },
];

// ── 净化规则 ────────────────────────────────────────────────
export const PURIFY_RULES: PurifyRule[] = [
  { id: "r1", name: "去除站点水印", pattern: "（.{0,12}(笔趣阁|www\\.[a-z0-9.]+).{0,12}）", replacement: "", isRegex: true, scope: "全部", enabled: true },
  { id: "r2", name: "去除翻页提示", pattern: "本章未完，请点击下一页继续阅读", replacement: "", isRegex: false, scope: "全部", enabled: true },
  { id: "r3", name: "去除求票尾巴", pattern: "(求月票|求推荐票|求收藏)[！!。]*", replacement: "", isRegex: true, scope: "起点中文网", enabled: true },
  { id: "r4", name: "统一省略号", pattern: "\\.{3,}|。{3,}", replacement: "……", isRegex: true, scope: "全部", enabled: false },
  { id: "r5", name: "替换敏感词占位", pattern: "\\*\\*\\*", replacement: "□□", isRegex: true, scope: "笔趣阁①", enabled: false },
];

// ── 正文生成（原创占位文本，含广告噪音用于演示净化规则） ──
const PARAS = [
  "夜色像一张被墨水浸透的旧纸，缓慢地铺满了整条长街。路灯一盏接一盏亮起来，光晕里浮着细小的雨丝。",
  "他站在书店门口，指尖还残留着纸页的触感。那本没有署名的旧书，此刻正安静地躺在他的外套口袋里。",
  "“你确定要打开它？”身后传来低低的声音。他没有回头，只是轻轻点了点头。",
  "钟楼的指针停在十一点五十九分，仿佛整座城市都在屏息等待着什么。风从巷口灌进来，带着潮湿的铁锈味。",
  "书页翻开的一瞬间，墨迹像是活了过来，在纸面上游走、聚拢，最终化作一行他从未见过的文字。",
  "（笔趣阁 www.biquge.example 最新章节免费阅读）",
  "他想起多年前那个下午，老人坐在藤椅上对他说过的话：真正的故事，从来不是写给所有人看的。",
  "雨越下越大，远处传来模糊的汽笛声。他把书合上，深吸了一口气，推门走进了那片更深的黑暗里。",
  "本章未完，请点击下一页继续阅读",
  "走廊尽头有一扇半掩的木门，门缝里透出昏黄的光。光里站着一个人影，似乎已经在那里等了很久。",
  "“你终于来了。”那人说。声音很轻，却像一枚石子，落进他心底最安静的湖面，激起一圈圈涟漪。",
  "他忽然明白，自己并不是偶然走到这里的。每一次犹豫，每一次转身，都早已写在那本书的某一页上。",
  "窗外的雨停了。月亮从云层后探出半张脸，把一地的积水照得像碎掉的镜子。",
  "求月票！求推荐票！",
  "他低头看向手中的书，最后一页依旧空白。可他知道，属于自己的那一行字，很快就会出现了。",
];

export function chapterContent(book: Book, idx: number): string[] {
  const seed = hash(book.id + ":" + idx);
  const count = 10 + (seed % 6);
  const out: string[] = [];
  for (let i = 0; i < count; i++) out.push(PARAS[(seed + i * 5) % PARAS.length]);
  if (book.kind === "local") return out.filter((p) => !/笔趣阁|本章未完|求月票/.test(p));
  return out;
}

export function applyPurify(text: string, rules: PurifyRule[], origin?: string): string {
  let t = text;
  for (const r of rules) {
    if (!r.enabled) continue;
    if (r.scope !== "全部" && r.scope !== origin) continue;
    try {
      t = r.isRegex ? t.replace(new RegExp(r.pattern, "g"), r.replacement) : t.split(r.pattern).join(r.replacement);
    } catch {
      /* 无效正则忽略 */
    }
  }
  return t;
}

// ── 阅读偏好 ────────────────────────────────────────────────
export interface PrefDef {
  key: string;
  label: string;
  desc: string;
  options: string[];
  section: string;
}

export const PREF_DEFS: PrefDef[] = [
  { section: "排版", key: "theme", label: "配色主题", desc: "阅读区域的前景/背景配色", options: ["终端默认", "护眼绿", "羊皮纸", "高对比"] },
  { section: "排版", key: "width", label: "行宽", desc: "每行最多显示的全角字符数", options: ["自适应", "28", "36", "44"] },
  { section: "排版", key: "spacing", label: "行距", desc: "终端中以空行模拟", options: ["1.0", "1.5", "2.0"] },
  { section: "排版", key: "indent", label: "段首缩进", desc: "段落前的全角空格数", options: ["0", "2"] },
  { section: "排版", key: "paraGap", label: "段间空行", desc: "段落之间是否插入空行", options: ["关", "开"] },
  { section: "翻页", key: "pageMode", label: "翻页方式", desc: "整页翻动或逐行滚动", options: ["整页", "滚动"] },
  { section: "翻页", key: "autoPage", label: "自动翻页", desc: "按间隔自动翻到下一页", options: ["关", "5s", "10s", "20s"] },
  { section: "翻页", key: "progress", label: "底部进度条", desc: "在状态栏显示章节进度", options: ["开", "关"] },
  { section: "内容", key: "purify", label: "启用净化规则", desc: "按「净化规则」处理正文", options: ["开", "关"] },
  { section: "内容", key: "chinese", label: "简繁转换", desc: "正文简繁体转换", options: ["关闭", "简→繁", "繁→简"] },
  { section: "内容", key: "preload", label: "预下载章节", desc: "后台缓存后续章节数", options: ["0", "5", "10", "30"] },
];

export type Prefs = Record<string, string>;

export const DEFAULT_PREFS: Prefs = {
  theme: "终端默认", width: "36", spacing: "1.5", indent: "2", paraGap: "关", pageMode: "整页",
  autoPage: "关", progress: "开", purify: "开", chinese: "关闭", preload: "10",
};

export const READER_THEMES: Record<string, { bg: string; fg: string; dim: string; accent: string }> = {
  终端默认: { bg: "#141414", fg: "#d6d6d6", dim: "#6e6e6e", accent: "#e3a35a" },
  护眼绿: { bg: "#142019", fg: "#bcd9b4", dim: "#5f7a63", accent: "#9fd49a" },
  羊皮纸: { bg: "#e8dcc2", fg: "#3a2f22", dim: "#8b7a5e", accent: "#9a5b1e" },
  高对比: { bg: "#000000", fg: "#ffffff", dim: "#9a9a9a", accent: "#ffd166" },
};

// 极简简繁映射（演示用）
const S2T: Record<string, string> = {
  书: "書", 门: "門", 来: "來", 说: "說", 话: "話", 这: "這", 个: "個", 时: "時", 间: "間", 后: "後",
  开: "開", 过: "過", 还: "還", 没: "沒", 灯: "燈", 钟: "鐘", 楼: "樓", 风: "風", 长: "長", 街: "街",
  经: "經", 头: "頭", 声: "聲", 远: "遠", 从: "從", 页: "頁", 边: "邊", 线: "線", 进: "進", 里: "裡",
  样: "樣", 轻: "輕", 终: "終", 现: "現", 飞: "飛", 见: "見", 亮: "亮", 纸: "紙", 带: "帶", 铁: "鐵",
  锈: "鏽", 墨: "墨", 迹: "跡", 聚: "聚", 从未: "從未", 为: "為", 属: "屬", 于: "於", 云: "雲", 层: "層",
  积: "積", 镜: "鏡", 碎: "碎", 犹: "猶", 豫: "豫", 转: "轉", 写: "寫", 涟: "漣", 漪: "漪", 湖: "湖",
};
export const toTraditional = (s: string) => s.replace(/[\u4e00-\u9fa5]/g, (c) => S2T[c] ?? c);
export const toSimplified = (s: string) => {
  const rev: Record<string, string> = {};
  Object.entries(S2T).forEach(([k, v]) => (rev[v] = k));
  return s.replace(/./g, (c) => rev[c] ?? c);
};

export const SPINNER = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
