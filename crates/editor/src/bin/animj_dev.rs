//! `animj-dev` —— animj 的无 GPU CLI（占位资产生成 + 关卡体检）。
//!
//! 契约：`docs/08-level-editor.md` §8.10（子命令、参数、输出、退出码）、
//! `docs/04-assets.md` §7（占位资产规格与调色板）。
//!
//! **硬约束**：本文件**不得**依赖 `bevy`（`08` §8.2.3 / §8.10.1）——该 bin 必须能在
//! 无显示器、无 GPU 的 CI 容器里运行，因此它只依赖 `level_format`、`std` 与 `png`（PNG 编码）。

mod exit {
    //! 退出码（`docs/08-level-editor.md` §8.10.4）。
    //!
    //! 文档约定该枚举位于 `crates/editor/src/dev/exit.rs`；`crates/editor` 没有 lib target，
    //! 因此这里以同内容的内联模块内聚在 `animj-dev` 的源文件里（不引入额外的 bin 目标）。

    /// CLI 退出码。**所有**路径都只经 [`exit`] 返回，禁止散落 `std::process::exit(数字)`。
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum ExitCode {
        /// 全部检查通过。
        Ok = 0,
        /// 内容不合格 / 占位资产漂移 / 零个匹配文件。
        CheckFailed = 1,
        /// 未知子命令、未知参数、参数值非法。
        UsageError = 2,
        /// 文件读不到、RON 语法错、结构无法反序列化、写盘失败。
        IoOrParseError = 3,
        /// 工具自身的不变式被破坏（如编码后回读不自洽）。
        InternalError = 4,
    }

    /// 退出进程。**这是唯一的 `std::process::exit` 调用点**（`08` §8.10.4）。
    pub fn exit(code: ExitCode) -> ! {
        std::process::exit(code as i32);
    }
}

mod palette {
    //! 占位资产调色板（8 色）。见 `docs/04-assets.md` §7.4。

    /// 直通 alpha 的 RGBA8 颜色。
    pub type Rgba = [u8; 4];

    /// `void` `#000000`：透明占位填充（实际写 `alpha = 0`）。
    pub const VOID: Rgba = [0x00, 0x00, 0x00, 0x00];
    /// `bg` `#1a1c2c`：底色。
    pub const BG: Rgba = [0x1a, 0x1c, 0x2c, 0xff];
    /// `primary` `#5d275d`：主体。
    pub const PRIMARY: Rgba = [0x5d, 0x27, 0x5d, 0xff];
    /// `secondary` `#b13e53`：次体。
    pub const SECONDARY: Rgba = [0xb1, 0x3e, 0x53, 0xff];
    /// `accent` `#ef7d57`：高亮。
    pub const ACCENT: Rgba = [0xef, 0x7d, 0x57, 0xff];
    /// `skin` `#ffcd75`：角色/装饰品基色。
    pub const SKIN: Rgba = [0xff, 0xcd, 0x75, 0xff];
    /// `marker` `#38b764`：帧号/索引标注。
    pub const MARKER: Rgba = [0x38, 0xb7, 0x64, 0xff];
    /// `outline` `#f4f4f4`：轮廓/文字。
    pub const OUTLINE: Rgba = [0xf4, 0xf4, 0xf4, 0xff];
}

mod raster {
    //! RGBA8 像素画布 + PNG 编码/回读自检。见 `docs/04-assets.md` §8（8-bit RGBA、无隔行）。

    use crate::palette::Rgba;

    /// 一张 RGBA8 画布；未显式绘制的像素保持全透明（`04` §7.4 的 `void`）。
    pub struct Raster {
        /// 宽（像素）。
        width: u32,
        /// 高（像素）。
        height: u32,
        /// 行主序 RGBA 字节。
        pixels: Vec<u8>,
    }

    impl Raster {
        /// 新建全透明画布。
        pub fn new(width: u32, height: u32) -> Self {
            Self {
                width,
                height,
                pixels: vec![0; width as usize * height as usize * 4],
            }
        }

        /// 写单个像素；越界坐标被忽略，便于按「贴图坐标」绘制时不必处处裁剪。
        pub fn set(&mut self, x: i32, y: i32, color: Rgba) {
            if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                return;
            }
            let offset = (y as usize * self.width as usize + x as usize) * 4;
            self.pixels[offset..offset + 4].copy_from_slice(&color);
        }

        /// 填充纯色矩形（不做抗锯齿/渐变，`04` §7.4）。
        pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: Rgba) {
            for dy in 0..height {
                for dx in 0..width {
                    self.set(x + dx, y + dy, color);
                }
            }
        }

        /// 画 1 像素宽的矩形描边。
        pub fn stroke_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: Rgba) {
            if width <= 0 || height <= 0 {
                return;
            }
            self.fill_rect(x, y, width, 1, color);
            self.fill_rect(x, y + height - 1, width, 1, color);
            self.fill_rect(x, y, 1, height, color);
            self.fill_rect(x + width - 1, y, 1, height, color);
        }
    }

    /// 编码为 PNG，并**立即解码回读逐字节比较**。
    ///
    /// 回读失败或字节不同 ⇒ `Err`，对应 `08` §8.10.4 的 `InternalError`（编解码不自洽 = 工具 bug）。
    pub fn encode_png(raster: &Raster) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, raster.width, raster.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            // 不做 DEFLATE 压缩：输出与压缩器实现无关，逐字节可复现（`04` §7.2 的确定性要求）。
            encoder.set_compression(png::Compression::NoCompression);
            let mut writer = encoder
                .write_header()
                .map_err(|e| format!("PNG 头写入失败：{e}"))?;
            writer
                .write_image_data(&raster.pixels)
                .map_err(|e| format!("PNG 像素数据写入失败：{e}"))?;
            writer.finish().map_err(|e| format!("PNG 收尾失败：{e}"))?;
        }
        verify_png(&bytes, &raster.pixels)?;
        Ok(bytes)
    }

    /// 回读校验：解码出的像素必须与源缓冲逐字节相同。
    fn verify_png(bytes: &[u8], expected: &[u8]) -> Result<(), String> {
        // `png::Decoder::new` 要求 `BufRead + Seek`，`&[u8]` 不实现 `Seek`，因此套一层 `Cursor`。
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("PNG 回读失败：{e}"))?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| String::from("PNG 回读失败：无法确定输出缓冲长度"))?;
        let mut decoded = vec![0u8; size];
        reader
            .next_frame(&mut decoded)
            .map_err(|e| format!("PNG 解码失败：{e}"))?;
        if decoded != expected {
            return Err(format!(
                "PNG 自检失败：回读 {} 字节，期望 {} 字节",
                decoded.len(),
                expected.len()
            ));
        }
        Ok(())
    }
}

mod glyph {
    //! 3×5 位图数字：仅用于占位图的索引/帧号标注。见 `docs/04-assets.md` §7.2。

    use crate::palette::{BG, OUTLINE, Rgba};
    use crate::raster::Raster;

    /// 单个字形宽（像素）。
    const GLYPH_W: i32 = 3;
    /// 单个字形高（像素）。
    const GLYPH_H: i32 = 5;
    /// 相邻字形的间距（像素）。
    const GLYPH_GAP: i32 = 1;
    /// 标注底衬宽（像素）：两位数字（3+1+3）+ 左右各 1 像素留白。
    const TAG_W: i32 = 9;
    /// 标注底衬高（像素）：字高 + 上下各 1 像素留白。
    const TAG_H: i32 = 7;

    /// `0..=9` 的 3×5 字形；每行 bit 2 = 最左列。
    const DIGIT_GLYPHS: [[u8; 5]; 10] = [
        [0b111, 0b101, 0b101, 0b101, 0b111], // 0
        [0b010, 0b110, 0b010, 0b010, 0b111], // 1
        [0b111, 0b001, 0b111, 0b100, 0b111], // 2
        [0b111, 0b001, 0b111, 0b001, 0b111], // 3
        [0b101, 0b101, 0b111, 0b001, 0b001], // 4
        [0b111, 0b100, 0b111, 0b001, 0b111], // 5
        [0b111, 0b100, 0b111, 0b101, 0b111], // 6
        [0b111, 0b001, 0b010, 0b010, 0b010], // 7
        [0b111, 0b101, 0b111, 0b101, 0b111], // 8
        [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    ];

    /// 在 `(x, y)` 处画单个数字字形。
    pub fn draw_digit(raster: &mut Raster, x: i32, y: i32, digit: u8, color: Rgba) {
        let Some(rows) = DIGIT_GLYPHS.get(digit as usize) else {
            return;
        };
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..GLYPH_W {
                if *bits & (1u8 << (GLYPH_W - 1 - column)) != 0 {
                    raster.set(x + column, y + row as i32, color);
                }
            }
        }
    }

    /// 在 `(x, y)` 处画 `n` 的十进制字形（左对齐，不含底衬）。
    pub fn draw_number(raster: &mut Raster, x: i32, y: i32, n: u32, color: Rgba) {
        let mut digits = [0u8; 10];
        let mut count = 0usize;
        let mut value = n;
        loop {
            digits[count] = (value % 10) as u8;
            count += 1;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        for position in 0..count {
            let digit = digits[count - 1 - position];
            draw_digit(
                raster,
                x + position as i32 * (GLYPH_W + GLYPH_GAP),
                y,
                digit,
                color,
            );
        }
    }

    /// 在 `(x, y)` 处画「底衬 + 索引数字」：占位图靠它做到「同尺寸不同格可肉眼区分」。
    pub fn draw_index_tag(raster: &mut Raster, x: i32, y: i32, n: u32) {
        raster.fill_rect(x, y, TAG_W, TAG_H, BG);
        draw_number(raster, x + 1, y + (TAG_H - GLYPH_H) / 2, n, OUTLINE);
    }
}

mod placeholder {
    //! 占位美术的程序化生成（纯函数：同参数 ⇒ 逐字节相同）。见 `docs/04-assets.md` §7。
    //!
    //! 全部图案都是纯色块（不做抗锯齿/渐变/半透明，`04` §7.4）；未使用的格保持**全透明**。

    use crate::glyph;
    use crate::palette::{ACCENT, BG, MARKER, OUTLINE, PRIMARY, Rgba, SECONDARY, SKIN, VOID};
    use crate::raster::Raster;
    use level_format::{RENDERABLE_TILE_KINDS, TILE_PX};

    /// 网格单元边长（像素）= `TILE_PX`。见 `docs/00-conventions.md` §0.3。
    const CELL_PX: i32 = TILE_PX;
    /// 角色图集列数。见 `docs/04-assets.md` §3.2 / §5.2。
    const PLAYER_SHEET_COLUMNS: u32 = 6;
    /// 角色图集行数。见 `docs/04-assets.md` §3.2 / §5.2。
    const PLAYER_SHEET_ROWS: u32 = 4;
    /// 每行动画的帧数，行序 = `idle / run / jump / fall`。见 `docs/04-assets.md` §5.2。
    const PLAYER_ANIM_FRAMES: [i32; PLAYER_SHEET_ROWS as usize] = [4, 6, 2, 2];
    /// 每行动画的占位基色（只影响观感，不改变帧语义）。见 `docs/04-assets.md` §7.4。
    const PLAYER_ROW_COLORS: [Rgba; PLAYER_SHEET_ROWS as usize] = [PRIMARY, SECONDARY, ACCENT, BG];
    /// 瓦片图集列数。见 `docs/04-assets.md` §10。
    const TILESET_COLUMNS: u32 = 8;
    /// 瓦片格占位色（按索引循环）。见 `docs/04-assets.md` §7.3。
    const TILE_COLORS: [Rgba; 4] = [PRIMARY, SECONDARY, ACCENT, SKIN];
    /// 装饰品中心图案色（按 id 指纹选色）。见 `docs/04-assets.md` §7.3。
    const DECORATION_MOTIF_COLORS: [Rgba; 4] = [PRIMARY, SECONDARY, ACCENT, MARKER];
    /// UI 图集列数。见 `docs/04-assets.md` §9。
    const UI_ATLAS_COLUMNS: u32 = 4;
    /// UI 图集行数。见 `docs/04-assets.md` §9。
    const UI_ATLAS_ROWS: u32 = 2;
    /// UI 格占位色（按索引循环）。见 `docs/04-assets.md` §7.4。
    const UI_CELL_COLORS: [Rgba; 8] = [
        BG, PRIMARY, SECONDARY, ACCENT, SKIN, MARKER, OUTLINE, PRIMARY,
    ];

    /// 角色图集：6 列 × 4 行 = 192×128；行 = `idle/run/jump/fall`，未用到的列全透明。
    /// 见 `docs/04-assets.md` §5.2 / §7.3。
    pub fn player_sheet() -> Raster {
        let mut raster = Raster::new(
            PLAYER_SHEET_COLUMNS * CELL_PX as u32,
            PLAYER_SHEET_ROWS * CELL_PX as u32,
        );
        for row in 0..PLAYER_SHEET_ROWS as i32 {
            for column in 0..PLAYER_ANIM_FRAMES[row as usize] {
                let (cell_x, cell_y) = (column * CELL_PX, row * CELL_PX);
                // 身体 = **碰撞盒尺寸**（20×28，底对齐）：占位图必须让"看到的就是撞到的"。
                // 曾经画满 32×32 ⇒ 贴墙时左右各 6 px、贴顶时上方 4 px 的视觉外沿被误认为穿模。
                let body_size = game::config::physics::PLAYER_SIZE;
                let (body_w, body_h) = (body_size.x as i32, body_size.y as i32);
                let body_x = cell_x + (CELL_PX - body_w) / 2;
                let body_y = cell_y + CELL_PX - body_h;
                raster.fill_rect(
                    body_x,
                    body_y,
                    body_w,
                    body_h,
                    PLAYER_ROW_COLORS[row as usize],
                );
                raster.stroke_rect(body_x, body_y, body_w, body_h, BG);
                raster.fill_rect(
                    cell_x + CELL_PX / 2 - 4,
                    cell_y + CELL_PX / 2 - 4,
                    8,
                    8,
                    SKIN,
                );
                // 帧序刻度：同一行内每帧刻度左端逐帧右移，肉眼可数。
                raster.fill_rect(cell_x + 3 + column * 4, cell_y + CELL_PX - 5, 3, 2, MARKER);
                let index = row as u32 * PLAYER_SHEET_COLUMNS + column as u32;
                glyph::draw_index_tag(&mut raster, cell_x + 2, cell_y + 2, index);
            }
            // 该行未用到的列显式写 `void`（全透明，不得留白花纹）：`04` §5.2。
            for column in PLAYER_ANIM_FRAMES[row as usize]..PLAYER_SHEET_COLUMNS as i32 {
                raster.fill_rect(column * CELL_PX, row * CELL_PX, CELL_PX, CELL_PX, VOID);
            }
        }
        raster
    }

    /// 音效占位：确定性 WAV（`04` §6.9）。`kind` 决定波形，`freq_hz` 与 `secs` 决定音高与长度。
    ///
    /// 纯函数：同参数 ⇒ 逐字节相同。
    pub fn wav_tone(freq_hz: f32, secs: f32, square: bool) -> Vec<u8> {
        const RATE: u32 = 44_100;
        let samples = (secs * RATE as f32).round().max(1.0) as u32;
        let mut pcm = Vec::with_capacity(samples as usize * 2);
        for i in 0..samples {
            let t = i as f32 / RATE as f32;
            let phase = (t * freq_hz) % 1.0;
            let raw = if square {
                if phase < 0.5 { 1.0 } else { -1.0 }
            } else {
                (phase * std::f32::consts::TAU).sin()
            };
            // 线性衰减，避免尾部爆音
            let envelope = 1.0 - (i as f32 / samples as f32);
            let value = (raw * envelope * 0.35 * i16::MAX as f32) as i16;
            pcm.extend_from_slice(&value.to_le_bytes());
        }
        let data_len = pcm.len() as u32;
        let mut out = Vec::with_capacity(44 + pcm.len());
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
        out.extend_from_slice(&2u16.to_le_bytes()); // block align
        out.extend_from_slice(&16u16.to_le_bytes()); // bits
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&pcm);
        out
    }

    /// 瓦片图集：`TILESET_COLUMNS` 列 × `N` 行（`N = ceil(可渲染条目数 / 列数)`），
    /// 索引 0 起依次是 `level_format::RENDERABLE_TILE_KINDS`；其余格全透明。
    /// 见 `docs/04-assets.md` §10 / §7.3。
    pub fn tileset() -> Raster {
        let rows = RENDERABLE_TILE_KINDS
            .len()
            .div_ceil(TILESET_COLUMNS as usize)
            .max(1);
        let mut raster = Raster::new(
            TILESET_COLUMNS * CELL_PX as u32,
            rows as u32 * CELL_PX as u32,
        );
        for (index, kind) in RENDERABLE_TILE_KINDS.iter().enumerate() {
            let column = (index % TILESET_COLUMNS as usize) as i32;
            let row = (index / TILESET_COLUMNS as usize) as i32;
            let (cell_x, cell_y) = (column * CELL_PX, row * CELL_PX);
            let color = TILE_COLORS[index % TILE_COLORS.len()];
            // 占位图必须让**物理语义**一眼可辨（实机反馈：`=` 与 `#` 画得一样，玩家以为穿模了）：
            //   `#` 实心 = 满格；`=` 单向平台 = 顶部薄板；`^` 尖刺 = 锯齿；装饰 = 内缩小块。
            match kind {
                level_format::TileKind::Solid => {
                    raster.fill_rect(cell_x, cell_y, CELL_PX, CELL_PX, color);
                    raster.stroke_rect(cell_x + 1, cell_y + 1, CELL_PX - 2, CELL_PX - 2, OUTLINE);
                }
                level_format::TileKind::OneWay => {
                    // **完整单格**：单向平台是隐藏通道（`01` §1.8），外观必须与普通地板一致，
                    // 否则"秘密"一眼就露。识别只靠图集里的索引数字与色调差异。
                    raster.fill_rect(cell_x, cell_y, CELL_PX, CELL_PX, color);
                    raster.stroke_rect(cell_x + 1, cell_y + 1, CELL_PX - 2, CELL_PX - 2, OUTLINE);
                }
                level_format::TileKind::Spike => {
                    // 3 个锯齿：每齿 4 层矩形台阶（纯色块，不做抗锯齿）
                    let tooth = CELL_PX / 3;
                    for tooth_index in 0..3 {
                        let base_x = cell_x + tooth_index * tooth;
                        for layer in 0..4 {
                            let inset = layer * (tooth / 8);
                            raster.fill_rect(
                                base_x + inset,
                                cell_y + CELL_PX - 6 - layer * 5,
                                tooth - inset * 2,
                                5,
                                color,
                            );
                        }
                    }
                }
                level_format::TileKind::Decor => {
                    raster.fill_rect(cell_x + 8, cell_y + 8, CELL_PX - 16, CELL_PX - 16, color);
                    raster.stroke_rect(
                        cell_x + 8,
                        cell_y + 8,
                        CELL_PX - 16,
                        CELL_PX - 16,
                        OUTLINE,
                    );
                }
                level_format::TileKind::Air => {}
            }
            glyph::draw_index_tag(&mut raster, cell_x + 2, cell_y + 2, index as u32);
        }
        // 索引 ≥ 可渲染条目数的格显式写 `void`（全透明）：`04` §10 规定它们不占语义。
        for index in RENDERABLE_TILE_KINDS.len()..rows * TILESET_COLUMNS as usize {
            let column = (index % TILESET_COLUMNS as usize) as i32;
            let row = (index / TILESET_COLUMNS as usize) as i32;
            raster.fill_rect(column * CELL_PX, row * CELL_PX, CELL_PX, CELL_PX, VOID);
        }
        raster
    }

    /// 单个装饰品贴图：32×32 单帧。`index` 是 id 在并集里的序号（用于角标，保证可区分）。
    /// 见 `docs/04-assets.md` §6.2 / §7.3。
    ///
    /// **背景必须全透明**：装饰品是叠加在角色上的图层（`04` §6.1），整格不透明会把角色
    /// 与其它图层完全遮住（全部 7 层同时装备时角色直接不可见）。
    pub fn decoration(id: &str, index: u32) -> Raster {
        let mut raster = Raster::new(CELL_PX as u32, CELL_PX as u32);
        let fingerprint = fingerprint(id);
        let motif = DECORATION_MOTIF_COLORS[fingerprint as usize % DECORATION_MOTIF_COLORS.len()];
        // 识别图形：居中的 12×12 实心方块 + 描边，四周保持透明。
        raster.fill_rect(CELL_PX / 2 - 6, CELL_PX / 2 - 6, 12, 12, motif);
        raster.stroke_rect(CELL_PX / 2 - 6, CELL_PX / 2 - 6, 12, 12, OUTLINE);
        // id 指纹条：8 个 2×3 的刻度，让不同 id 即使序号相同也能靠图案区分；灭的刻度保持透明。
        let bits = fingerprint ^ (fingerprint >> 13);
        for bit in 0..8 {
            if (bits >> bit) & 1 == 1 {
                raster.fill_rect(3 + bit * 3, CELL_PX - 6, 2, 3, motif);
            }
        }
        glyph::draw_index_tag(&mut raster, 2, 2, index);
        raster
    }

    /// UI 图集：4 列 × 2 行 = 128×64。见 `docs/04-assets.md` §9（M0 只需确定性的占位格）。
    pub fn ui_atlas() -> Raster {
        let mut raster = Raster::new(
            UI_ATLAS_COLUMNS * CELL_PX as u32,
            UI_ATLAS_ROWS * CELL_PX as u32,
        );
        for row in 0..UI_ATLAS_ROWS as i32 {
            for column in 0..UI_ATLAS_COLUMNS as i32 {
                let (cell_x, cell_y) = (column * CELL_PX, row * CELL_PX);
                let index = row as u32 * UI_ATLAS_COLUMNS + column as u32;
                let color = UI_CELL_COLORS[index as usize % UI_CELL_COLORS.len()];
                raster.fill_rect(cell_x, cell_y, CELL_PX, CELL_PX, color);
                raster.stroke_rect(cell_x + 2, cell_y + 2, CELL_PX - 4, CELL_PX - 4, OUTLINE);
                glyph::draw_index_tag(&mut raster, cell_x + 2, cell_y + 2, index + 2);
            }
        }
        raster
    }

    /// FNV-1a 32 位指纹：把 id 确定性地映射到图案参数（只用于肉眼区分，非安全用途）。
    fn fingerprint(id: &str) -> u32 {
        let mut hash: u32 = 0x811c_9dc5;
        for byte in id.as_bytes() {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use crate::exit::ExitCode;

/// 默认资产根目录（相对仓库根）。见 `docs/08-level-editor.md` §8.10.3 的 `--out` 默认值。
const DEFAULT_ASSETS_DIR: &str = "assets";
/// 关卡目录名（位于资产根下）。见 `docs/00-conventions.md` §0.6 与 `08` §8.10.1。
const LEVELS_DIR_NAME: &str = "levels";
/// 关卡文件后缀。见 `docs/03-level-format.md` §3.1。
const LEVEL_FILE_SUFFIX: &str = ".level.ron";

/// 角色图集输出路径（相对资产根）。见 `docs/04-assets.md` §1 / §3.3。
const PLAYER_SHEET_PATH: &str = "sprites/player/cat_sheet.png";
/// 瓦片图集输出路径（相对资产根）。见 `docs/04-assets.md` §1 / §3.3。
const TILESET_PATH: &str = "sprites/tiles/tileset.png";
/// 装饰品贴图目录（相对资产根）。见 `docs/04-assets.md` §1 / §6。
const DECORATIONS_DIR: &str = "sprites/decorations";
/// UI 图集输出路径（相对资产根）。见 `docs/04-assets.md` §1 / §9。
const UI_ATLAS_PATH: &str = "sprites/ui/ui_atlas.png";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    exit::exit(run(&args));
}

/// 唯一的命令分派点；返回值即进程退出码（`08` §8.10.4）。
fn run(args: &[String]) -> ExitCode {
    let Some(subcommand) = args.first() else {
        eprintln!("animj-dev: 缺少子命令\n");
        eprint!("{}", usage_text());
        return ExitCode::UsageError;
    };
    match subcommand.as_str() {
        "gen-placeholder-assets" => cmd_gen_placeholder_assets(&args[1..]),
        "validate-levels" => cmd_validate_levels(&args[1..]),
        "--help" | "-h" => {
            print!("{}", usage_text());
            ExitCode::Ok
        }
        "--version" | "-V" => {
            println!("animj-dev {}", env!("CARGO_PKG_VERSION"));
            ExitCode::Ok
        }
        other => {
            eprintln!("animj-dev: 未知子命令 `{other}`\n");
            eprint!("{}", usage_text());
            ExitCode::UsageError
        }
    }
}

/// 用法文本（`--help` 与用法错误共用）。
fn usage_text() -> String {
    String::from(
        "animj-dev —— animj 的 CI 工具（无 GPU；见 docs/08-level-editor.md §8.10）\n\
         \n\
         USAGE:\n\
         \x20 animj-dev gen-placeholder-assets [--out <dir>] [--force] [--check]\n\
         \x20 animj-dev validate-levels [<path>...] [--dir <dir>] [--json] [--deny-warnings] [--strict] [--format-version <n>]\n\
         \x20 animj-dev --help | --version\n\
         \n\
         EXIT CODES:\n\
         \x20 0 ok   1 check failed   2 usage error   3 I/O or parse error   4 internal error\n",
    )
}

// ---------------------------------------------------------------------------
// 通用工具
// ---------------------------------------------------------------------------

/// 统一用 `/` 分隔符打印路径：stdout 必须与操作系统无关地稳定可比（`08` §8.10.1「同输入同输出」）。
fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 一个路径是否是关卡文件（按后缀判定，见 `03` §3.1）。
fn is_level_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(LEVEL_FILE_SUFFIX))
}

/// 关卡 id 段 = 文件名去掉 `.level.ron`。见 `docs/03-level-format.md` §3.1。
fn level_id_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    name.strip_suffix(LEVEL_FILE_SUFFIX).map(str::to_owned)
}

/// 递归收集 `dir` 下的全部 `.level.ron`，按路径字典序返回（顺序确定，便于比对）。
fn collect_level_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current)
            .map_err(|e| format!("读取目录 {} 失败：{e}", display_path(&current)))?;
        let mut children: Vec<PathBuf> = entries
            .map(|entry| entry.map(|entry| entry.path()).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("读取目录 {} 失败：{e}", display_path(&current)))?;
        children.sort();
        for child in children {
            if child.is_dir() {
                stack.push(child);
            } else if is_level_file(&child) {
                files.push(child);
            }
        }
    }
    files.sort();
    Ok(files)
}

// ---------------------------------------------------------------------------
// gen-placeholder-assets
// ---------------------------------------------------------------------------

/// `gen-placeholder-assets` 的选项。见 `docs/08-level-editor.md` §8.10.3。
struct GenOptions {
    /// 输出根（= 资产根 `assets/`）。
    out: PathBuf,
    /// 覆盖已存在的文件（默认跳过，保护手工替换过的真美术）。
    force: bool,
    /// 只比对不写盘：任何差异 ⇒ 退出码 1（CI 防漂移）。
    check: bool,
}

/// 解析 `gen-placeholder-assets` 的参数；任何未知参数/缺值 ⇒ `UsageError`。
fn parse_gen_options(args: &[String]) -> Result<GenOptions, ExitCode> {
    let mut options = GenOptions {
        out: PathBuf::from(DEFAULT_ASSETS_DIR),
        force: false,
        check: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            // `--help` 在任意位置都打印用法并以 0 退出（`08` §8.10.4）；用 `Err(Ok)` 作为「已处理」信号。
            "--help" | "-h" => {
                print!("{}", usage_text());
                return Err(ExitCode::Ok);
            }
            "--out" => {
                let Some(value) = args.get(i + 1) else {
                    eprintln!("animj-dev: `--out` 缺少参数值");
                    return Err(ExitCode::UsageError);
                };
                options.out = PathBuf::from(value);
                i += 2;
            }
            "--force" => {
                options.force = true;
                i += 1;
            }
            "--check" => {
                options.check = true;
                i += 1;
            }
            other => {
                eprintln!("animj-dev: 未知参数 `{other}`");
                return Err(ExitCode::UsageError);
            }
        }
    }
    Ok(options)
}

/// `gen-placeholder-assets`：按 `04` §7.3 输出全部占位 PNG。
fn cmd_gen_placeholder_assets(args: &[String]) -> ExitCode {
    let options = match parse_gen_options(args) {
        Ok(options) => options,
        Err(code) => return code,
    };

    // 装饰品 id 清单的唯一来源 = 全部关卡 `reward` 的并集（`04` §7.3）。
    let ids = match collect_reward_ids(&options.out.join(LEVELS_DIR_NAME)) {
        Ok(ids) => ids,
        Err(reason) => {
            eprintln!("animj-dev: {reason}");
            return ExitCode::IoOrParseError;
        }
    };

    // 装饰品 id 必须是 `[a-z][a-z0-9_]*`（`04` §2）：否则会生成路径非法的贴图文件，直接判失败。
    let invalid: Vec<&str> = ids
        .iter()
        .filter(|id| !is_valid_decoration_id(id))
        .map(|id| id.as_str())
        .collect();
    if !invalid.is_empty() {
        for id in &invalid {
            eprintln!(
                "animj-dev: 关卡 reward `{id}` 不是合法的装饰品 id（规则见 docs/04-assets.md §2）"
            );
        }
        return ExitCode::CheckFailed;
    }

    // 生成（纯函数）+ 逐张回读自检；自检失败说明编码不自洽 ⇒ `08` §8.10.4 的 InternalError。
    let jobs = match plan_jobs(&ids) {
        Ok(jobs) => jobs,
        Err(reason) => {
            eprintln!("animj-dev: 内部错误：{reason}");
            return ExitCode::InternalError;
        }
    };

    let mut written = 0usize;
    let mut skipped = 0usize;
    let mut drift = 0usize;
    for (relative, bytes) in &jobs {
        let full = options.out.join(relative);
        if options.check {
            match fs::read(&full) {
                Ok(existing) if existing == *bytes => {}
                Ok(_) => {
                    println!("drift {}", display_path(&full));
                    drift += 1;
                }
                Err(_) => {
                    println!("missing {}", display_path(&full));
                    drift += 1;
                }
            }
            continue;
        }
        if full.exists() && !options.force {
            println!("skip {}", display_path(&full));
            skipped += 1;
            continue;
        }
        if let Some(parent) = full.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            eprintln!("animj-dev: 无法创建目录 {}：{e}", display_path(parent));
            return ExitCode::IoOrParseError;
        }
        if let Err(e) = fs::write(&full, bytes) {
            eprintln!("animj-dev: 无法写入 {}：{e}", display_path(&full));
            return ExitCode::IoOrParseError;
        }
        println!("write {}", display_path(&full));
        written += 1;
    }

    if options.check {
        if drift > 0 {
            println!("{drift} 个文件与生成器不一致，请重新运行 `animj-dev gen-placeholder-assets`");
            return ExitCode::CheckFailed;
        }
        println!(
            "check ok: 占位资产与生成器逐字节一致（{} 个文件）",
            jobs.len()
        );
        return ExitCode::Ok;
    }
    println!("{written} written, {skipped} skipped");
    ExitCode::Ok
}

/// 扫描 `dir` 下全部关卡并取其 `reward` 并集（字典序，去掉重复）。见 `04` §7.3。
///
/// 目录不存在 = 没有关卡 = 没有装饰品 id（不是错误）；但**关卡文件读不懂**是错误：
/// 此时 id 集合无法确定，必须报告而不是静默生成残缺资产。
fn collect_reward_ids(dir: &Path) -> Result<Vec<String>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut ids = std::collections::BTreeSet::new();
    for file in collect_level_files(dir)? {
        let text = fs::read_to_string(&file)
            .map_err(|e| format!("读取 {} 失败：{e}", display_path(&file)))?;
        let level = level_format::read(&text)
            .map_err(|e| format!("解析 {} 失败：{e}", display_path(&file)))?;
        for room in &level.rooms {
            if let Some(reward) = &room.reward {
                ids.insert(reward.clone());
            }
        }
    }
    Ok(ids.into_iter().collect())
}

/// 装饰品 id 规则：`[a-z][a-z0-9_]*`。见 `docs/04-assets.md` §2（与 `00` §0.5 一致）。
fn is_valid_decoration_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// 生成计划：`(相对资产根的路径, PNG 字节)`。顺序固定（先图集，再按 id 字典序的装饰品，最后 UI）。
fn plan_jobs(ids: &[String]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut jobs = Vec::with_capacity(ids.len() + 10);
    jobs.push((
        PLAYER_SHEET_PATH.to_owned(),
        raster::encode_png(&placeholder::player_sheet())?,
    ));
    jobs.push((
        TILESET_PATH.to_owned(),
        raster::encode_png(&placeholder::tileset())?,
    ));
    for (index, id) in ids.iter().enumerate() {
        let raster = placeholder::decoration(id, index as u32);
        jobs.push((
            format!("{}/{id}.png", DECORATIONS_DIR),
            raster::encode_png(&raster)?,
        ));
    }
    jobs.push((
        UI_ATLAS_PATH.to_owned(),
        raster::encode_png(&placeholder::ui_atlas())?,
    ));
    // 音效占位（`04` §6.9）：cue → (频率, 时长, 方波?)
    for (cue, freq, secs, square) in [
        ("jump", 660.0, 0.09, true),
        ("land", 220.0, 0.06, true),
        ("bump", 140.0, 0.05, true),
        ("pickup", 990.0, 0.07, false),
        ("clear", 880.0, 0.18, false),
        ("decoration", 1320.0, 0.12, false),
        ("death", 110.0, 0.20, true),
    ] {
        jobs.push((
            format!("audio/{cue}.wav"),
            placeholder::wav_tone(freq, secs, square),
        ));
    }
    Ok(jobs)
}

// ---------------------------------------------------------------------------
// validate-levels
// ---------------------------------------------------------------------------

/// `validate-levels` 的选项。见 `docs/08-level-editor.md` §8.10.2。
struct ValidateOptions {
    /// 显式给出的文件/目录；为空时回落到默认 `assets/levels`。
    targets: Vec<PathBuf>,
    /// 只输出一个 JSON 文档到 stdout。
    json: bool,
    /// 把 `Warning` 也计为失败（`--strict` 是它的别名）。
    deny_warnings: bool,
    /// 额外断言每个文件的 `version` 等于给定值。
    format_version: Option<u32>,
}

/// 解析 `validate-levels` 的参数；任何未知参数/非法值 ⇒ `UsageError`。
fn parse_validate_options(args: &[String]) -> Result<ValidateOptions, ExitCode> {
    let mut options = ValidateOptions {
        targets: Vec::new(),
        json: false,
        deny_warnings: false,
        format_version: None,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            // `--help` 在任意位置都打印用法并以 0 退出（`08` §8.10.4）；用 `Err(Ok)` 作为「已处理」信号。
            "--help" | "-h" => {
                print!("{}", usage_text());
                return Err(ExitCode::Ok);
            }
            "--dir" => {
                let Some(value) = args.get(i + 1) else {
                    eprintln!("animj-dev: `--dir` 缺少参数值");
                    return Err(ExitCode::UsageError);
                };
                options.targets.push(PathBuf::from(value));
                i += 2;
            }
            "--json" => {
                options.json = true;
                i += 1;
            }
            "--deny-warnings" | "--strict" => {
                options.deny_warnings = true;
                i += 1;
            }
            "--format-version" => {
                let Some(value) = args.get(i + 1) else {
                    eprintln!("animj-dev: `--format-version` 缺少参数值");
                    return Err(ExitCode::UsageError);
                };
                match value.parse::<u32>() {
                    Ok(version) => options.format_version = Some(version),
                    Err(_) => {
                        eprintln!("animj-dev: `--format-version` 的值 `{value}` 不是合法的版本号");
                        return Err(ExitCode::UsageError);
                    }
                }
                i += 2;
            }
            other if other.starts_with('-') => {
                eprintln!("animj-dev: 未知参数 `{other}`");
                return Err(ExitCode::UsageError);
            }
            path => {
                options.targets.push(PathBuf::from(path));
                i += 1;
            }
        }
    }
    if options.targets.is_empty() {
        options
            .targets
            .push(PathBuf::from(DEFAULT_ASSETS_DIR).join(LEVELS_DIR_NAME));
    }
    Ok(options)
}

/// CLI 侧的问题严重度（与 `level_format::Severity` 一一对应，输出层不引入上游类型）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Severity {
    /// 结构不变式被破坏。
    Fatal,
    /// 语义违规。
    Error,
    /// 可疑但可用。
    Warning,
}

/// 把上游严重度原样映射到 CLI 侧。
fn severity_of(value: level_format::Severity) -> Severity {
    match value {
        level_format::Severity::Fatal => Severity::Fatal,
        level_format::Severity::Error => Severity::Error,
        level_format::Severity::Warning => Severity::Warning,
    }
}

impl Severity {
    /// 人类可读行里的标签（`08` §8.10.2）。
    fn label(self) -> &'static str {
        match self {
            Severity::Fatal => "FATAL",
            Severity::Error => "ERROR",
            Severity::Warning => "WARN ",
        }
    }

    /// `--json` 里的标签（`08` §8.10.2）。
    fn json_label(self) -> &'static str {
        match self {
            Severity::Fatal => "fatal",
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// 读取 `key[index]` 形式的括号下标：返回下标值与「`]` 之后」的剩余文本。
fn bracket_index<'a>(text: &'a str, key: &str) -> Option<(usize, &'a str)> {
    let start = text.find(key)?;
    let rest = text.get(start + key.len()..)?.strip_prefix('[')?;
    let end = rest.find(']')?;
    let index = rest.get(..end)?.parse::<usize>().ok()?;
    Some((index, rest.get(end + 1..)?))
}

/// 从 issue 的 `path`（`03` §3.7 的字段路径，如 `rooms[13].goals[1].count`）反解房间坐标。
///
/// 房间下标是 `rooms` 的行主序下标（`room_index(col, row, cols) = row * cols + col`），
/// 因此 `(col, row) = (i % cols, i / cols)`。瓦片类 path 在本版本只有 `rooms[i].tiles[row]`
/// （带文件行号、不带列），故不产出瓦片坐标。
fn room_of_path(path: &str, cols: u32) -> Option<(u32, u32)> {
    let (index, _) = bracket_index(path, "rooms")?;
    if cols == 0 {
        return None;
    }
    let index = index as u32;
    Some((index % cols, index / cols))
}

/// 一个文件里的一条 issue 的 CLI 侧投影。
struct IssueLine {
    severity: Severity,
    /// 从 `path` 反解出的房间坐标（`--json` 的 `room` 字段）。
    room: Option<(u32, u32)>,
    message: String,
}

/// 一个文件的体检结果。
struct FileReport {
    /// 打印用路径（`/` 分隔、相对当前工作目录）。
    display: String,
    /// `Fatal` + `Error` 的条数（决定「内容不合格」）。
    violations: usize,
    /// 全部 issue（含 `Warning`）。
    issues: Vec<IssueLine>,
    /// `Some` 表示该文件读不到或读不懂（`08` §8.10.4 的退出码 3）。
    read_error: Option<String>,
}

impl FileReport {
    /// 该文件是否「不合格」（计入失败文件数）。
    fn failed(&self, deny_warnings: bool) -> bool {
        self.read_error.is_some()
            || self.violations > 0
            || (deny_warnings && self.issues.len() > self.violations)
    }
}

/// `validate-levels`：对每个关卡文件跑「读文件 → RON 解析 → 版本检查 → `validate`」。
fn cmd_validate_levels(args: &[String]) -> ExitCode {
    let options = match parse_validate_options(args) {
        Ok(options) => options,
        Err(code) => return code,
    };

    let mut files = Vec::new();
    for target in &options.targets {
        if target.is_dir() {
            match collect_level_files(target) {
                Ok(mut found) => files.append(&mut found),
                Err(reason) => {
                    eprintln!("animj-dev: {reason}");
                    return ExitCode::IoOrParseError;
                }
            }
        } else if target.is_file() {
            files.push(target.clone());
        } else {
            eprintln!("animj-dev: 路径不存在：{}", display_path(target));
            return ExitCode::IoOrParseError;
        }
    }
    files.sort();
    files.dedup();

    if files.is_empty() {
        eprintln!(
            "animj-dev: 未找到任何 `{LEVEL_FILE_SUFFIX}` 文件（扫描 {}）",
            options
                .targets
                .iter()
                .map(|path| display_path(path))
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::CheckFailed;
    }

    let reports: Vec<FileReport> = files
        .iter()
        .map(|file| inspect_level_file(file, &options))
        .collect();

    let unreadable = reports
        .iter()
        .filter(|report| report.read_error.is_some())
        .count();
    let failed = reports
        .iter()
        .filter(|report| report.failed(options.deny_warnings))
        .count();
    let ok = reports.len() - failed;
    let errors = reports
        .iter()
        .map(|report| report.violations)
        .sum::<usize>();
    let warnings = reports
        .iter()
        .map(|report| report.issues.len() - report.violations)
        .sum::<usize>();

    // 读不到/读不懂的原因始终写 stderr（`08` §8.10.1 的「错误/诊断到 stderr」），
    // 这样 `--json` 模式下 stdout 仍然只有一个 JSON 文档。
    for report in &reports {
        if let Some(reason) = &report.read_error {
            eprintln!("animj-dev: {}: {reason}", report.display);
        }
    }

    if options.json {
        print_json(&reports, ok, failed, errors, warnings);
    } else {
        for report in &reports {
            let verdict = if report.failed(options.deny_warnings) {
                "FAIL"
            } else {
                "OK"
            };
            println!("{}: {verdict}", report.display);
            for issue in &report.issues {
                // `level_format` 的 message 已含「Vxx 编号 + 字段路径 + 描述」（`03` §3.7 的对外契约），
                // 因此这里不再重复拼 path，避免同一路径出现两次。
                println!("  {} {}", issue.severity.label(), issue.message);
            }
        }
        let mut summary = format!(
            "{} files checked: {ok} ok, {failed} failed ({errors} errors, {warnings} warnings)",
            reports.len()
        );
        if unreadable > 0 {
            summary.push_str(&format!(", {unreadable} unreadable"));
        }
        println!("{summary}");
    }

    // 退出码优先级：读不到/读不懂（环境或数据污染）优先于内容不合格，二者在有任何单一原因时互斥。
    if unreadable > 0 {
        ExitCode::IoOrParseError
    } else if failed > 0 {
        ExitCode::CheckFailed
    } else {
        ExitCode::Ok
    }
}

/// 体检单个关卡文件。
fn inspect_level_file(file: &Path, options: &ValidateOptions) -> FileReport {
    let display = display_path(file);
    let mut report = FileReport {
        display,
        violations: 0,
        issues: Vec::new(),
        read_error: None,
    };

    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => {
            report.read_error = Some(format!("读取失败：{e}"));
            return report;
        }
    };
    let level = match level_format::read(&text) {
        Ok(level) => level,
        Err(e) => {
            report.read_error = Some(format!("解析失败：{e}"));
            return report;
        }
    };

    // 上游校验（唯一规则来源，`08` §8.8.2）。
    for issue in level_format::validate(&level) {
        let severity = severity_of(issue.severity);
        let room = room_of_path(&issue.path, level.grid.cols);
        report.issues.push(IssueLine {
            severity,
            room,
            message: issue.message,
        });
    }

    // 文件名一致性（V2 的后半段）：`validate` 是 `Level` 上的纯函数，拿不到文件名，所以在这里补。
    if let Some(level_id) = level_id_of(file)
        && !level_format::name_matches_file(&level, &level_id)
    {
        report.issues.push(IssueLine {
            severity: Severity::Error,
            room: None,
            message: format!(
                "V2 name: 关卡名 `{}` 与文件名主干 `{level_id}` 不一致",
                level.name
            ),
        });
    }

    // `--format-version <n>`：发布前的批量版本体检（`08` §8.10.2）。
    if let Some(expected) = options.format_version
        && level.version != expected
    {
        report.issues.push(IssueLine {
            severity: Severity::Error,
            room: None,
            message: format!(
                "V1 version: 文件版本 {}，期望 {}（--format-version）",
                level.version, expected
            ),
        });
    }

    report.violations = report
        .issues
        .iter()
        .filter(|issue| matches!(issue.severity, Severity::Fatal | Severity::Error))
        .count();
    report
}

/// `--json`：stdout 上只输出一个 JSON 文档（字段顺序固定，`08` §8.10.2）。
///
/// 读不到/读不懂的文件也进 `files`（`ok: false`，并带一条 `severity: "error"` 的诊断条目），
/// 这样 CI 不需要同时看 stderr 就能统计；具体原因仍会写到 stderr。
fn print_json(reports: &[FileReport], ok: usize, failed: usize, errors: usize, warnings: usize) {
    let mut out = String::from("{\n  \"files\": [\n");
    for (index, report) in reports.iter().enumerate() {
        let file_ok = report.read_error.is_none() && report.violations == 0;
        out.push_str("    {\n");
        out.push_str(&format!(
            "      \"path\": \"{}\",\n",
            json_escape(&report.display)
        ));
        out.push_str(&format!("      \"ok\": {file_ok},\n"));
        out.push_str("      \"issues\": [");
        let mut first = true;
        if let Some(reason) = &report.read_error {
            out.push_str(&format!(
                "\n        {{ \"severity\": \"error\", \"room\": null, \"tile\": null, \"message\": \"{}\" }}",
                json_escape(reason)
            ));
            first = false;
        }
        for issue in &report.issues {
            if !first {
                out.push(',');
            }
            first = false;
            let room = match issue.room {
                Some((col, row)) => format!("[{col},{row}]"),
                None => String::from("null"),
            };
            // `03` §3.7 的瓦片类 path 形如 `rooms[i].tiles[row]`（带文件行、不带列），
            // 无法还原出 `(x, y)` 的瓦片坐标，因此该字段恒为 `null`（列信息只在 message 里）。
            out.push_str(&format!(
                "\n        {{ \"severity\": \"{}\", \"room\": {room}, \"tile\": null, \"message\": \"{}\" }}",
                issue.severity.json_label(),
                json_escape(&issue.message)
            ));
        }
        if !first {
            out.push('\n');
            out.push_str("      ");
        }
        out.push_str("]\n    }");
        if index + 1 < reports.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");
    out.push_str(&format!(
        "  \"summary\": {{ \"files\": {}, \"ok\": {ok}, \"failed\": {failed}, \"errors\": {errors}, \"warnings\": {warnings} }}\n",
        reports.len()
    ));
    out.push_str("}\n");
    print!("{out}");
}

/// JSON 字符串转义（手写 JSON 只此一处需要转义）。
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32))
            }
            other => out.push(other),
        }
    }
    out
}
