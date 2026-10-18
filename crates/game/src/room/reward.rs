//! 装饰品奖励的授予。所有者文档：`docs/01-game-design.md` §1.5。
//!
//! **幂等**：同一装饰品只授予一次；`RoomCleared` 在一个房间最多发一次（`progress::mark_cleared`），
//! 但这里仍然按"拥有集合"去重，避免热重载等路径造成的重复。

use bevy::prelude::*;

use super::{DecorationGranted, RoomCleared};
use crate::player::decor::Decorations;
use crate::world::load::LoadedLevel;

pub fn grant_rewards(
    mut cleared: MessageReader<RoomCleared>,
    loaded: Option<Res<LoadedLevel>>,
    mut decorations: ResMut<Decorations>,
    mut granted: MessageWriter<DecorationGranted>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    for event in cleared.read() {
        let Some(room) = loaded.level.rooms.get(event.room_index as usize) else {
            continue;
        };
        let Some(reward) = room.reward.clone() else {
            continue;
        };
        if decorations.grant(reward.clone()) {
            info!("授予装饰品：{reward}");
            granted.write(DecorationGranted {
                decoration_id: reward,
            });
        }
    }
}
