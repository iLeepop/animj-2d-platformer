//! 瓦片字符 ↔ `TileKind` 的固定 legend。见 `docs/03-level-format.md` §3.3

/// 瓦片类型。字符、碰撞语义与图集索引见 `docs/03-level-format.md` §3.3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileKind {
    /// `.` 空：不渲染、不生成实体、无碰撞。
    Air,
    /// `#` 实心地形：四面阻挡。
    Solid,
    /// `=` 单向平台：仅当下落且脚底此前在平台上沿之上时可站。
    OneWay,
    /// `^` 危险：无阻挡，玩家 AABB 相交即死亡。
    Spike,
    /// `*` 无碰撞装饰，渲染在贴花层。
    Decor,
}

/// legend 表：字符 ↔ 瓦片类型。顺序即 `docs/03-level-format.md` §3.3 表的顺序，不支持按文件自定义。
pub const LEGEND: &[(char, TileKind)] = &[
    ('.', TileKind::Air),
    ('#', TileKind::Solid),
    ('=', TileKind::OneWay),
    ('^', TileKind::Spike),
    ('*', TileKind::Decor),
];

/// 需要渲染的瓦片类型，顺序即 `tileset.png` 的行主序索引 `0..=3`。
/// `Air` 不占图集格，故不在其中。见 `docs/03-level-format.md` §3.3
pub const RENDERABLE_TILE_KINDS: &[TileKind] = &[
    TileKind::Solid,
    TileKind::OneWay,
    TileKind::Spike,
    TileKind::Decor,
];

impl TileKind {
    /// 解析 legend 字符；不属于 legend 的字符返回 `None`。
    pub fn from_char(c: char) -> Option<TileKind> {
        LEGEND
            .iter()
            .find(|(legend_char, _)| *legend_char == c)
            .map(|(_, kind)| *kind)
    }

    /// 本类型的 legend 字符；与 `LEGEND` 一一对应。
    pub fn to_char(self) -> char {
        match self {
            TileKind::Air => '.',
            TileKind::Solid => '#',
            TileKind::OneWay => '=',
            TileKind::Spike => '^',
            TileKind::Decor => '*',
        }
    }

    /// 在瓦片图集中的行主序索引；`Air` 不渲染，返回 `None`。
    pub fn atlas_index(self) -> Option<usize> {
        RENDERABLE_TILE_KINDS.iter().position(|kind| *kind == self)
    }

    /// 是否四面阻挡。
    pub fn is_solid(self) -> bool {
        self == TileKind::Solid
    }
}
