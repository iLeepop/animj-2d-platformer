//! 2D 粒子/特效。**所有者文档：`docs/07-rendering.md` §7.14**。
//!
//! 纯表现层：粒子不参与物理、不写 `Aabb`、不影响权威状态；服务端/单机是权威观感，
//! 联机客户端不要求逐帧一致（见 `07` §7.14.6）。
//!
//! 结构：
//! - `ParticleBurst` 消息是**唯一**的生成入口（`FixedUpdate` 的跳跃/落地与 `Update` 的拾取/存档/buff 都写它）；
//! - [`spawn_bursts`] 在 `GameSet::Fx` 消费消息并生成实体；
//! - [`update_particles`] 负责积分、序列帧、淡出与销毁；
//! - 粒子挂世界根（`07` §7.8.2），带 `PixelSnap`（`07` §7.6.4），Z 取 `00` §0.4 的粒子子带。

use bevy::prelude::*;
use bevy::sprite::Anchor;
use level_format::{ItemKind, TILE_PX};

use crate::checkpoint::{ACTIVE_COLOR, CheckpointActivated};
use crate::config::net::MAX_PLAYERS;
use crate::config::particle as cfg;
use crate::config::render::z;
use crate::physics::StepOutcome;
use crate::player::PlayerId;
use crate::player::items::{CarriedItem, ItemPicked, item_color};
use crate::player::{PlayerMotion, PlayerStepOutcome};
use crate::render::pixel_camera::PIXEL_PERFECT_LAYERS;
use crate::render::snap::PixelSnap;
use crate::state::GameSet;
use crate::world::load::LoadedLevel;

/// 一次粒子爆发的种类。`Item` / `Buff` 携带道具类型，用于取色与发射参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleKind {
    /// 起跳烟尘（脚底）。
    Jump,
    /// 二段跳烟尘（脚底）。
    AirJump,
    /// 落地烟尘（脚底）。
    Land,
    /// 拾取道具的火花（玩家中心）。
    Item(ItemKind),
    /// 激活存档点的火花（存档点中心）。
    Checkpoint,
    /// buff 持续期间的角色跟随粒子（玩家中心）。
    Buff(ItemKind),
}

/// 请求一次粒子爆发。**唯一**生成入口；见 `docs/05-architecture.md` §5.6。
#[derive(Message, Debug, Clone, Copy)]
pub struct ParticleBurst {
    pub kind: ParticleKind,
    /// 世界像素坐标；`Jump/AirJump/Land` 传脚底中点，其余传实体中心。
    pub origin: Vec2,
}

/// 一个粒子实体。字段只由本模块读写。
#[derive(Component, Debug)]
pub struct Particle {
    /// 已存活时间（秒）。
    age: f32,
    /// 总寿命（秒）。
    life: f32,
    /// 当前速度（像素/秒）。
    vel: Vec2,
    /// 重力（像素/秒²）。
    gravity: f32,
    /// 线性阻力（每秒衰减比例）。
    drag: f32,
    /// 原始颜色（淡出只改 alpha）。
    color: Color,
    /// 序列帧播放帧率。
    fps: f32,
    /// 序列帧总数。
    frames: u32,
}

/// 两张序列帧贴图之一。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sheet {
    Dust,
    Spark,
}

/// 一次爆发的静态参数。
#[derive(Debug, Clone, Copy)]
struct Preset {
    burst: cfg::BurstParams,
    sheet: Sheet,
    /// 锚点（底部或中心）。
    anchor: Vec2,
    color: Color,
    fps: f32,
    frames: u32,
}

/// 单粒子的随机结果（纯数据，便于测试）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct BurstSpec {
    offset: Vec2,
    velocity: Vec2,
    life: f32,
}

/// 粒子贴图句柄（启动时构建一次）。
#[derive(Resource)]
struct ParticleAssets {
    dust: Handle<Image>,
    dust_layout: Handle<TextureAtlasLayout>,
    spark: Handle<Image>,
    spark_layout: Handle<TextureAtlasLayout>,
}

impl ParticleAssets {
    fn sheet(&self, sheet: Sheet) -> (Handle<Image>, Handle<TextureAtlasLayout>) {
        match sheet {
            Sheet::Dust => (self.dust.clone(), self.dust_layout.clone()),
            Sheet::Spark => (self.spark.clone(), self.spark_layout.clone()),
        }
    }
}

/// 全局生成序号：既用于 Z 子带，也作为确定性随机种子。
#[derive(Resource, Default)]
struct ParticleSeq(u32);

pub struct ParticlePlugin;

impl Plugin for ParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleSeq>()
            .add_message::<ParticleBurst>()
            .add_systems(Startup, setup_particle_assets)
            // 跳跃/落地在**固定步**里产生：每步都读一次 `PlayerStepOutcome`，不会像 `Changed`
            // 那样在一帧多步时丢掉中间那一步（`07` §7.14.3）。
            .add_systems(
                FixedUpdate,
                emit_player_bursts
                    .after(crate::player::apply_player_physics)
                    .in_set(GameSet::Physics)
                    .run_if(crate::net::not_client),
            )
            .add_systems(
                Update,
                (
                    (emit_item_bursts, emit_checkpoint_bursts, emit_buff_aura).chain(),
                    (spawn_bursts, update_particles).chain(),
                )
                    .chain()
                    .in_set(GameSet::Fx),
            );
    }
}

fn setup_particle_assets(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let cell = UVec2::splat(TILE_PX as u32);
    let dust = assets.load(cfg::DUST_IMAGE);
    let dust_layout = layouts.add(TextureAtlasLayout::from_grid(
        cell,
        cfg::DUST_FRAMES,
        1,
        None,
        None,
    ));
    let spark = assets.load(cfg::SPARK_IMAGE);
    let spark_layout = layouts.add(TextureAtlasLayout::from_grid(
        cell,
        cfg::SPARK_FRAMES,
        1,
        None,
        None,
    ));
    commands.insert_resource(ParticleAssets {
        dust,
        dust_layout,
        spark,
        spark_layout,
    });
}

/// 固定步内把 `PlayerStepOutcome` 变成爆发请求。
fn emit_player_bursts(
    players: Query<(&PlayerMotion, &PlayerStepOutcome)>,
    mut bursts: MessageWriter<ParticleBurst>,
) {
    for (motion, outcome) in &players {
        let origin = motion.0.aabb.min;
        for kind in player_burst_kinds(&outcome.0) {
            bursts.write(ParticleBurst { kind, origin });
        }
    }
}

/// 一步物理结果 ⇒ 要播放的爆发（纯函数，可单测）。
fn player_burst_kinds(outcome: &StepOutcome) -> Vec<ParticleKind> {
    let mut kinds = Vec::new();
    if outcome.jumped {
        kinds.push(ParticleKind::Jump);
    }
    if outcome.air_jumped {
        kinds.push(ParticleKind::AirJump);
    }
    if outcome.landed {
        kinds.push(ParticleKind::Land);
    }
    kinds
}

/// 道具拾取 → 在玩家中心爆发（`ItemPicked` 由主机/单机写出）。
fn emit_item_bursts(
    players: Query<(&PlayerId, &PlayerMotion)>,
    mut picked: MessageReader<ItemPicked>,
    mut bursts: MessageWriter<ParticleBurst>,
) {
    for event in picked.read() {
        let Some((_, motion)) = players.iter().find(|(id, _)| id.0 == event.player.0) else {
            continue;
        };
        bursts.write(ParticleBurst {
            kind: ParticleKind::Item(event.kind),
            origin: motion.0.aabb.center(),
        });
    }
}

/// 存档点激活 → 在存档点中心爆发。
fn emit_checkpoint_bursts(
    loaded: Option<Res<LoadedLevel>>,
    mut activated: MessageReader<CheckpointActivated>,
    mut bursts: MessageWriter<ParticleBurst>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    let rows = loaded.level.grid.rows;
    for event in activated.read() {
        bursts.write(ParticleBurst {
            kind: ParticleKind::Checkpoint,
            origin: event.point.world_center(rows),
        });
    }
}

/// buff 持续粒子：每个玩家一个计时器，按 `kind` 取不同发射参数。
///
/// 拾取瞬间的爆发由 `ItemPicked` 负责；这里只做“生效期间”的跟随粒子，因此不会
/// 在拾取那一帧重复爆发。客户端也能读到快照同步的 `CarriedItem`，所以持续粒子在
/// 客户端同样可见（一次性爆发不要求一致，`07` §7.14.6）。
fn emit_buff_aura(
    time: Res<Time>,
    players: Query<(&PlayerId, &PlayerMotion, &CarriedItem)>,
    mut bursts: MessageWriter<ParticleBurst>,
    mut states: Local<[AuraState; MAX_PLAYERS]>,
) {
    let dt = time.delta_secs();
    for (id, motion, carried) in &players {
        let index = id.index().min(MAX_PLAYERS - 1);
        let state = &mut states[index];
        let Some(kind) = aura_due(state, carried.kind, dt) else {
            continue;
        };
        bursts.write(ParticleBurst {
            kind: ParticleKind::Buff(kind),
            origin: motion.0.aabb.center() + Vec2::new(0.0, 6.0),
        });
    }
}

/// 每个玩家的 buff 持续粒子计时状态。
#[derive(Debug, Clone, Copy, Default)]
struct AuraState {
    timer: f32,
    kind: Option<ItemKind>,
}

/// 消费 [`ParticleBurst`] 并生成粒子实体。
fn spawn_bursts(
    mut commands: Commands,
    assets: Res<ParticleAssets>,
    mut seq: ResMut<ParticleSeq>,
    particles: Query<(), With<Particle>>,
    mut bursts: MessageReader<ParticleBurst>,
) {
    let mut active = particles.iter().count();
    for burst in bursts.read() {
        let preset = preset(burst.kind);
        let room = cfg::MAX_PARTICLES.saturating_sub(active);
        let count = preset.burst.count.min(cfg::MAX_BURST).min(room as u32);
        if count == 0 {
            continue;
        }
        let specs = burst_specs(burst.kind, seq.0 as u64);
        let (image, layout) = assets.sheet(preset.sheet);
        for spec in specs.into_iter().take(count as usize) {
            let z = z_for(seq.0);
            seq.0 = seq.0.wrapping_add(1);
            let mut sprite = Sprite::from_atlas_image(
                image.clone(),
                TextureAtlas {
                    layout: layout.clone(),
                    index: 0,
                },
            );
            sprite.color = preset.color;
            commands.spawn((
                sprite,
                Anchor(preset.anchor),
                Transform::from_xyz(
                    burst.origin.x + spec.offset.x,
                    burst.origin.y + spec.offset.y,
                    z,
                ),
                PIXEL_PERFECT_LAYERS,
                PixelSnap,
                Particle {
                    age: 0.0,
                    life: spec.life,
                    vel: spec.velocity,
                    gravity: preset.burst.gravity,
                    drag: preset.burst.drag,
                    color: preset.color,
                    fps: preset.fps,
                    frames: preset.frames,
                },
            ));
        }
        active += count as usize;
        if count > 1 {
            info!("粒子：{:?} ×{}（当前 {}）", burst.kind, count, active);
        } else {
            debug!("粒子：{:?} ×1（当前 {}）", burst.kind, active);
        }
    }
}

/// 积分、序列帧、淡出与销毁。
fn update_particles(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Particle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs().min(cfg::MAX_DT);
    for (entity, mut particle, mut transform, mut sprite) in &mut query {
        particle.age += dt;
        if particle.age >= particle.life {
            commands.entity(entity).despawn();
            continue;
        }
        particle.vel.y += particle.gravity * dt;
        let damping = (1.0 - particle.drag * dt).max(0.0);
        particle.vel *= damping;
        transform.translation.x += particle.vel.x * dt;
        transform.translation.y += particle.vel.y * dt;

        let base = particle.color.to_srgba();
        sprite.color = Color::srgba(
            base.red,
            base.green,
            base.blue,
            fade_alpha(particle.age, particle.life),
        );

        let index = frame_index(particle.age, particle.fps, particle.frames);
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = index;
        }
    }
}

/// 寿命 ⇒ alpha（线性淡出，钳制在 `[0, 1]`）。
fn fade_alpha(age: f32, life: f32) -> f32 {
    if life <= 0.0 {
        return 0.0;
    }
    (1.0 - age / life).clamp(0.0, 1.0)
}

/// 年龄 ⇒ 序列帧下标（非循环：最后一帧保持到销毁）。
fn frame_index(age: f32, fps: f32, frames: u32) -> usize {
    if frames == 0 {
        return 0;
    }
    let frame = (age.max(0.0) * fps.max(0.0)) as u32;
    frame.min(frames - 1) as usize
}

/// 粒子 Z：`00` §0.4 的粒子子带 `[60.0, 60.9)`，按生成序循环取不同值。
fn z_for(seq: u32) -> f32 {
    z::PARTICLE + (seq % cfg::Z_SLOTS) as f32 * cfg::Z_STEP
}

/// 爆发预置（纯函数）。
fn preset(kind: ParticleKind) -> Preset {
    let bottom = Vec2::new(0.5, 0.0);
    let center = Vec2::new(0.5, 0.5);
    match kind {
        ParticleKind::Jump => Preset {
            burst: cfg::JUMP,
            sheet: Sheet::Dust,
            anchor: bottom,
            color: cfg::DUST_COLOR,
            fps: cfg::FRAME_FPS_DUST,
            frames: cfg::DUST_FRAMES,
        },
        ParticleKind::AirJump => Preset {
            burst: cfg::AIR_JUMP,
            sheet: Sheet::Dust,
            anchor: bottom,
            color: cfg::DUST_COLOR,
            fps: cfg::FRAME_FPS_DUST,
            frames: cfg::DUST_FRAMES,
        },
        ParticleKind::Land => Preset {
            burst: cfg::LAND,
            sheet: Sheet::Dust,
            anchor: bottom,
            color: cfg::DUST_COLOR,
            fps: cfg::FRAME_FPS_DUST,
            frames: cfg::DUST_FRAMES,
        },
        ParticleKind::Item(kind) => Preset {
            burst: cfg::ITEM,
            sheet: Sheet::Spark,
            anchor: center,
            color: item_color(kind),
            fps: cfg::FRAME_FPS_SPARK,
            frames: cfg::SPARK_FRAMES,
        },
        ParticleKind::Checkpoint => Preset {
            burst: cfg::CHECKPOINT,
            sheet: Sheet::Spark,
            anchor: center,
            color: ACTIVE_COLOR,
            fps: cfg::FRAME_FPS_SPARK,
            frames: cfg::SPARK_FRAMES,
        },
        ParticleKind::Buff(kind) => Preset {
            burst: aura_params(kind),
            sheet: Sheet::Spark,
            anchor: center,
            color: item_color(kind),
            fps: cfg::FRAME_FPS_SPARK,
            frames: cfg::SPARK_FRAMES,
        },
    }
}

/// 三种 buff 的持续粒子参数（`01` §1.5：能力修正见 `06` §6.12）。
fn aura_params(kind: ItemKind) -> cfg::BurstParams {
    match kind {
        ItemKind::SuperJump => cfg::AURA_SUPER_JUMP,
        ItemKind::Speed => cfg::AURA_SPEED,
        ItemKind::DoubleJump => cfg::AURA_DOUBLE_JUMP,
    }
}

/// 确定性爆发采样（纯函数；同 seed ⇒ 同结果）。
fn burst_specs(kind: ParticleKind, seed: u64) -> Vec<BurstSpec> {
    let preset = preset(kind);
    let mut rng = Rng::new(seed ^ kind_salt(kind));
    (0..preset.burst.count)
        .map(|_| {
            let angle = rng
                .range(preset.burst.angle.0, preset.burst.angle.1)
                .to_radians();
            let speed = rng.range(preset.burst.speed.0, preset.burst.speed.1);
            let offset_radius = preset.burst.offset;
            let offset = if preset.anchor.y <= 0.25 {
                Vec2::new(
                    rng.range(-offset_radius, offset_radius),
                    rng.range(0.0, offset_radius),
                )
            } else {
                Vec2::new(
                    rng.range(-offset_radius, offset_radius),
                    rng.range(-offset_radius, offset_radius),
                )
            };
            BurstSpec {
                offset,
                velocity: Vec2::new(angle.cos(), angle.sin()) * speed,
                life: preset.burst.life,
            }
        })
        .collect()
}

/// 每种粒子的固定随机盐，避免不同效果的序列完全相同。
fn kind_salt(kind: ParticleKind) -> u64 {
    match kind {
        ParticleKind::Jump => 1,
        ParticleKind::AirJump => 2,
        ParticleKind::Land => 3,
        ParticleKind::Checkpoint => 4,
        ParticleKind::Item(kind) => 10 + kind.code() as u64,
        ParticleKind::Buff(kind) => 20 + kind.code() as u64,
    }
}

/// 小型确定性 RNG（xorshift64*），避免引入 `rand`（`09` §3.2）。
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
}

/// buff 持续粒子的计时规则（纯函数）：只有 Some 且到间隔才返回该道具类型。
fn aura_due(state: &mut AuraState, kind: Option<ItemKind>, dt: f32) -> Option<ItemKind> {
    let Some(kind) = kind else {
        *state = AuraState::default();
        return None;
    };
    if state.kind != Some(kind) {
        *state = AuraState {
            timer: cfg::BUFF_AURA_INTERVAL,
            kind: Some(kind),
        };
    }
    state.timer -= dt;
    if state.timer <= 0.0 {
        state.timer += cfg::BUFF_AURA_INTERVAL;
        Some(kind)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn all_kinds() -> Vec<ParticleKind> {
        vec![
            ParticleKind::Jump,
            ParticleKind::AirJump,
            ParticleKind::Land,
            ParticleKind::Item(ItemKind::SuperJump),
            ParticleKind::Item(ItemKind::Speed),
            ParticleKind::Item(ItemKind::DoubleJump),
            ParticleKind::Checkpoint,
            ParticleKind::Buff(ItemKind::SuperJump),
            ParticleKind::Buff(ItemKind::Speed),
            ParticleKind::Buff(ItemKind::DoubleJump),
        ]
    }

    fn test_assets() -> ParticleAssets {
        ParticleAssets {
            dust: Handle::default(),
            dust_layout: Handle::default(),
            spark: Handle::default(),
            spark_layout: Handle::default(),
        }
    }

    fn particle_count(app: &mut App) -> usize {
        let mut query = app.world_mut().query_filtered::<Entity, With<Particle>>();
        query.iter(app.world()).count()
    }

    /// 所有预置都必须在预算内，且参数区间合法。
    #[test]
    fn presets_fit_the_budget_and_have_valid_ranges() {
        for kind in all_kinds() {
            let preset = preset(kind);
            assert!(preset.burst.count > 0, "{kind:?}");
            assert!(preset.burst.count <= cfg::MAX_BURST, "{kind:?}");
            assert!(preset.burst.life > 0.0, "{kind:?}");
            assert!(preset.burst.speed.0 <= preset.burst.speed.1, "{kind:?}");
            assert!(preset.burst.angle.0 <= preset.burst.angle.1, "{kind:?}");
            assert!(preset.frames > 0, "{kind:?}");
        }
    }

    /// 同 seed 的采样完全一致；不同 seed 必须不同。
    #[test]
    fn burst_sampling_is_deterministic() {
        assert_eq!(
            burst_specs(ParticleKind::Jump, 7),
            burst_specs(ParticleKind::Jump, 7)
        );
        assert_ne!(
            burst_specs(ParticleKind::Jump, 7),
            burst_specs(ParticleKind::Jump, 8)
        );
    }

    /// 每颗粒子的速度与偏移都不越出预置范围。
    #[test]
    fn burst_specs_stay_inside_the_preset() {
        for kind in all_kinds() {
            let preset = preset(kind);
            let specs = burst_specs(kind, 123);
            assert_eq!(specs.len(), preset.burst.count as usize, "{kind:?}");
            for spec in specs {
                let speed = spec.velocity.length();
                assert!(
                    speed >= preset.burst.speed.0 - 1e-3 && speed <= preset.burst.speed.1 + 1e-3,
                    "{kind:?} speed = {speed}"
                );
                assert!(spec.offset.x.abs() <= preset.burst.offset + 1e-3);
                assert!(spec.offset.y.abs() <= preset.burst.offset + 1e-3);
                assert_eq!(spec.life, preset.burst.life);
            }
        }
    }

    #[test]
    fn frame_index_clamps_to_the_sheet() {
        assert_eq!(frame_index(0.0, 12.0, 4), 0);
        assert_eq!(frame_index(0.09, 12.0, 4), 1);
        assert_eq!(frame_index(10.0, 12.0, 4), 3);
        assert_eq!(frame_index(-1.0, 12.0, 4), 0);
        assert_eq!(frame_index(1.0, 12.0, 0), 0);
    }

    #[test]
    fn alpha_fades_from_one_to_zero() {
        assert_eq!(fade_alpha(0.0, 0.5), 1.0);
        assert_eq!(fade_alpha(0.25, 0.5), 0.5);
        assert_eq!(fade_alpha(0.5, 0.5), 0.0);
        assert_eq!(fade_alpha(1.0, 0.5), 0.0);
        assert_eq!(fade_alpha(0.0, 0.0), 0.0);
    }

    /// 粒子必须落在 `00` §0.4 预留的子带里，且循环复用。
    #[test]
    fn z_stays_inside_the_particle_band() {
        assert_eq!(z_for(0), z::PARTICLE);
        for seq in 0..cfg::Z_SLOTS * 2 {
            let z = z_for(seq);
            assert!(z >= z::PARTICLE, "seq = {seq}");
            assert!(
                z < z::PARTICLE + cfg::Z_SLOTS as f32 * cfg::Z_STEP,
                "seq = {seq}"
            );
            assert!(z < crate::config::render::z::TILE_FOREGROUND, "seq = {seq}");
        }
        assert_eq!(z_for(cfg::Z_SLOTS), z_for(0));
    }

    /// 物理结果映射：起跳/二段跳/落地各自一簇，空结果不发射。
    #[test]
    fn player_outcome_maps_to_the_expected_bursts() {
        let jumped = StepOutcome {
            jumped: true,
            ..Default::default()
        };
        assert_eq!(player_burst_kinds(&jumped), vec![ParticleKind::Jump]);

        let air = StepOutcome {
            air_jumped: true,
            ..Default::default()
        };
        assert_eq!(player_burst_kinds(&air), vec![ParticleKind::AirJump]);

        let landed = StepOutcome {
            landed: true,
            ..Default::default()
        };
        assert_eq!(player_burst_kinds(&landed), vec![ParticleKind::Land]);

        assert!(player_burst_kinds(&StepOutcome::default()).is_empty());
    }

    /// 三种 buff 的持续粒子颜色必须可区分（与 `item_color` 同源）。
    #[test]
    fn buff_aura_colors_match_the_item_palette() {
        let colors = [
            preset(ParticleKind::Buff(ItemKind::SuperJump)).color,
            preset(ParticleKind::Buff(ItemKind::Speed)).color,
            preset(ParticleKind::Buff(ItemKind::DoubleJump)).color,
        ];
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
        assert_ne!(colors[0], colors[2]);
        assert_eq!(colors[0], item_color(ItemKind::SuperJump));
        assert_eq!(preset(ParticleKind::Checkpoint).color, ACTIVE_COLOR);
    }

    #[test]
    fn spawns_particles_and_expires_them() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .insert_resource(test_assets())
            .init_resource::<ParticleSeq>()
            .add_message::<ParticleBurst>()
            .add_systems(Update, (spawn_bursts, update_particles).chain());

        app.world_mut().write_message(ParticleBurst {
            kind: ParticleKind::Jump,
            origin: Vec2::new(10.0, 20.0),
        });
        app.update();
        assert!(particle_count(&mut app) > 0);

        for _ in 0..20 {
            app.update();
        }
        assert_eq!(particle_count(&mut app), 0);
    }

    #[test]
    fn global_cap_is_enforced() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_assets())
            .init_resource::<ParticleSeq>()
            .add_message::<ParticleBurst>()
            .add_systems(Update, (spawn_bursts, update_particles).chain());

        for _ in 0..30 {
            app.world_mut().write_message(ParticleBurst {
                kind: ParticleKind::Jump,
                origin: Vec2::ZERO,
            });
        }
        app.update();
        let count = particle_count(&mut app);
        assert!(count > 0);
        assert!(count <= cfg::MAX_PARTICLES, "count = {count}");
    }

    /// 固定步的 `jumped` 必须经 `ParticleBurst` 变成真实粒子（触发链路，而非只测纯函数）。
    #[test]
    fn player_outcome_spawns_particles_through_the_message() {
        let mut app = App::new();
        app.add_message::<ParticleBurst>()
            .insert_resource(test_assets())
            .init_resource::<ParticleSeq>()
            .add_systems(Update, (emit_player_bursts, spawn_bursts).chain());
        app.world_mut().spawn((
            PlayerMotion(crate::physics::Body::at_center(Vec2::new(100.0, 100.0))),
            PlayerStepOutcome(StepOutcome {
                jumped: true,
                ..Default::default()
            }),
        ));
        app.update();
        assert!(particle_count(&mut app) > 0);
    }

    /// `ItemPicked` 必须映射到道具类型的爆发，且落在玩家中心。
    #[test]
    fn item_pickup_spawns_particles_through_the_message() {
        let mut app = App::new();
        app.add_message::<ItemPicked>()
            .add_message::<ParticleBurst>()
            .insert_resource(test_assets())
            .init_resource::<ParticleSeq>()
            .add_systems(Update, (emit_item_bursts, spawn_bursts).chain());
        app.world_mut().spawn((
            PlayerId(0),
            PlayerMotion(crate::physics::Body::at_center(Vec2::new(100.0, 100.0))),
        ));
        app.world_mut().write_message(ItemPicked {
            player: PlayerId(0),
            kind: ItemKind::Speed,
            replaced: None,
        });
        app.update();
        assert!(particle_count(&mut app) > 0);
    }

    /// `CheckpointActivated` 必须映射到存档点中心的一次爆发。
    #[test]
    fn checkpoint_activation_spawns_particles_through_the_message() {
        let mut app = App::new();
        app.add_message::<CheckpointActivated>()
            .add_message::<ParticleBurst>()
            .insert_resource(test_assets())
            .init_resource::<ParticleSeq>()
            .insert_resource(LoadedLevel {
                level: level_format::Level {
                    version: level_format::VERSION,
                    name: "t".into(),
                    grid: level_format::Grid { cols: 1, rows: 1 },
                    start: level_format::RoomCoord { col: 0, row: 0 },
                    tileset: None,
                    tileset_columns: 8,
                    palette: Vec::new(),
                    background: None,
                    items: Vec::new(),
                    rooms: vec![level_format::Room {
                        name: None,
                        spawn: None,
                        tiles: vec![".".repeat(level_format::ROOM_TILES); level_format::ROOM_TILES],
                        entities: Vec::new(),
                    }],
                },
                handle: Handle::default(),
            })
            .add_systems(Update, (emit_checkpoint_bursts, spawn_bursts).chain());
        app.world_mut().write_message(CheckpointActivated {
            point: crate::checkpoint::CheckpointPoint {
                col: 0,
                row: 0,
                at: (16, 16),
            },
        });
        app.update();
        assert!(particle_count(&mut app) > 0);
    }

    /// buff 的持续粒子在计时到期前不发射，到期后发射一次。
    #[test]
    fn buff_aura_waits_for_the_interval() {
        let mut state = AuraState::default();
        let kind = ItemKind::Speed;
        // 首次看到 kind：只初始化计时器，不发射。
        assert_eq!(aura_due(&mut state, Some(kind), 0.0), None);
        assert_eq!(
            aura_due(&mut state, Some(kind), cfg::BUFF_AURA_INTERVAL * 0.5),
            None
        );
        assert_eq!(
            aura_due(&mut state, Some(kind), cfg::BUFF_AURA_INTERVAL * 0.6),
            Some(kind)
        );
        assert_eq!(aura_due(&mut state, None, 0.0), None);
        assert_eq!(state.kind, None);
    }
}
