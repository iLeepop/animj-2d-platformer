//! 轴对齐包围盒。所有者文档：`docs/06-physics-and-movement.md` §6.5.1。
//!
//! **逻辑真相是 `Aabb`，不是 `Transform`**（`00` §0.3、`06` §6.8.1）。

use bevy::math::Vec2;

/// 世界像素坐标系下的轴对齐包围盒（`min.x < max.x && min.y < max.y`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    /// 由中心与尺寸构造（关卡文件的 `at` 就是 AABB 中心，见 `00` §0.3）。
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let half = size * 0.5;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn translate(&self, delta: Vec2) -> Self {
        Self {
            min: self.min + delta,
            max: self.max + delta,
        }
    }

    /// 四边各向内收缩 `inset`（`inset > 0`）或向外扩张（`inset < 0`）。
    ///
    /// Hazard 判定用内缩（`HAZARD_INSET`），拾取判定用外扩（`PICKUP_INFLATE`），见 `06` §6.5.3/§6.5.4。
    pub fn inset(&self, inset: f32) -> Self {
        let d = Vec2::splat(inset);
        Self {
            min: self.min + d,
            max: self.max - d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_center_size_round_trips() {
        let aabb = Aabb::from_center_size(Vec2::new(100.0, 50.0), Vec2::new(20.0, 28.0));
        assert_eq!(aabb.min, Vec2::new(90.0, 36.0));
        assert_eq!(aabb.max, Vec2::new(110.0, 64.0));
        assert_eq!(aabb.center(), Vec2::new(100.0, 50.0));
        assert_eq!(aabb.size(), Vec2::new(20.0, 28.0));
    }

    #[test]
    fn inset_shrinks_and_grows() {
        let aabb = Aabb::from_center_size(Vec2::ZERO, Vec2::new(20.0, 28.0));
        assert_eq!(aabb.inset(2.0).size(), Vec2::new(16.0, 24.0));
        assert_eq!(aabb.inset(-2.0).size(), Vec2::new(24.0, 32.0));
        assert_eq!(aabb.inset(0.0), aabb);
    }
}
