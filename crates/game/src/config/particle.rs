//! 粒子/特效常量。**所有者文档：`docs/07-rendering.md` §7.14**。
//!
//! 资产规格（32×32 格、4 帧、透明底）归 `docs/04-assets.md` §3.3 / §7.3；
//! 本文件只放**运行时行为**的数值，避免两处各写一套。

use bevy::prelude::Color;

/// 跳跃/落地烟尘贴图（相对 `assets/`）。规格见 `docs/04-assets.md` §3.3。
pub const DUST_IMAGE: &str = "sprites/effects/dust.png";
/// 拾取/存档/buff 火花贴图。规格见 `docs/04-assets.md` §3.3。
pub const SPARK_IMAGE: &str = "sprites/effects/spark.png";
/// 烟尘序列帧数（`04` §3.3）。
pub const DUST_FRAMES: u32 = 4;
/// 火花序列帧数（`04` §3.3）。
pub const SPARK_FRAMES: u32 = 4;
/// 烟尘序列帧播放帧率（帧/秒）。见 `07` §7.14。
pub const FRAME_FPS_DUST: f32 = 12.0;
/// 火花序列帧播放帧率（帧/秒）。见 `07` §7.14。
pub const FRAME_FPS_SPARK: f32 = 15.0;

/// 同屏粒子总数上限。见 `07` §7.14。
pub const MAX_PARTICLES: usize = 192;
/// 单条爆发消息最多生成的粒子数。见 `07` §7.14。
pub const MAX_BURST: u32 = 24;

/// 单帧积分步长上限（秒）：切后台/断点后避免粒子瞬移。见 `07` §7.14。
pub const MAX_DT: f32 = 1.0 / 30.0;

/// 粒子 Z 子带的步长。见 `00` §0.4。
pub const Z_STEP: f32 = 0.001;
/// 粒子 Z 子带槽位数（循环复用）。见 `00` §0.4。
pub const Z_SLOTS: u32 = 900;

/// buff 持续粒子的发射间隔（秒）。见 `07` §7.14。
pub const BUFF_AURA_INTERVAL: f32 = 0.15;

/// 烟尘的基色（贴图为近白，运行时按效果染色）。见 `07` §7.14。
pub const DUST_COLOR: Color = Color::srgb(0.93, 0.93, 0.87);

/// 一次爆发/单粒子的参数。见 `docs/07-rendering.md` §7.14。
#[derive(Debug, Clone, Copy)]
pub struct BurstParams {
    /// 本次生成的粒子数。
    pub count: u32,
    /// 初速度范围（像素/秒）。
    pub speed: (f32, f32),
    /// 速度方向范围（度；0 = 右，90 = 上）。
    pub angle: (f32, f32),
    /// 寿命（秒）。
    pub life: f32,
    /// 重力（像素/秒²，负值向下）。
    pub gravity: f32,
    /// 线性阻力（每秒衰减比例）。
    pub drag: f32,
    /// 生成点抖动半径（像素）；底部锚点只向上抖。
    pub offset: f32,
}

/// 起跳烟尘。见 `07` §7.14。
pub const JUMP: BurstParams = BurstParams {
    count: 10,
    speed: (45.0, 95.0),
    angle: (25.0, 155.0),
    life: 0.42,
    gravity: -260.0,
    drag: 1.6,
    offset: 5.0,
};

/// 二段跳烟尘（`StepOutcome.air_jumped`）。见 `07` §7.14。
pub const AIR_JUMP: BurstParams = BurstParams {
    count: 8,
    speed: (35.0, 75.0),
    angle: (30.0, 150.0),
    life: 0.35,
    gravity: -240.0,
    drag: 1.8,
    offset: 5.0,
};

/// 落地烟尘。见 `07` §7.14。
pub const LAND: BurstParams = BurstParams {
    count: 8,
    speed: (25.0, 60.0),
    angle: (10.0, 170.0),
    life: 0.32,
    gravity: -180.0,
    drag: 2.4,
    offset: 6.0,
};

/// 拾取道具的火花。见 `07` §7.14。
pub const ITEM: BurstParams = BurstParams {
    count: 14,
    speed: (70.0, 150.0),
    angle: (0.0, 360.0),
    life: 0.50,
    gravity: -80.0,
    drag: 1.0,
    offset: 3.0,
};

/// 激活存档点的火花。见 `07` §7.14。
pub const CHECKPOINT: BurstParams = BurstParams {
    count: 18,
    speed: (60.0, 140.0),
    angle: (0.0, 360.0),
    life: 0.65,
    gravity: -60.0,
    drag: 0.8,
    offset: 3.0,
};

/// `SuperJump` buff 的持续粒子：向上飘的余烬。见 `07` §7.14。
pub const AURA_SUPER_JUMP: BurstParams = BurstParams {
    count: 1,
    speed: (20.0, 45.0),
    angle: (70.0, 110.0),
    life: 0.60,
    gravity: -10.0,
    drag: 0.5,
    offset: 6.0,
};

/// `Speed` buff 的持续粒子：向四周散开的拖尾。见 `07` §7.14。
pub const AURA_SPEED: BurstParams = BurstParams {
    count: 1,
    speed: (35.0, 65.0),
    angle: (0.0, 360.0),
    life: 0.35,
    gravity: 0.0,
    drag: 3.0,
    offset: 6.0,
};

/// `DoubleJump` buff 的持续粒子：环绕的星尘。见 `07` §7.14。
pub const AURA_DOUBLE_JUMP: BurstParams = BurstParams {
    count: 1,
    speed: (15.0, 40.0),
    angle: (0.0, 360.0),
    life: 0.70,
    gravity: -30.0,
    drag: 0.6,
    offset: 7.0,
};
