# AGENTS.md

本仓库的完整开发规范在 **[`docs/AGENTS.md`](docs/AGENTS.md)**。入口索引在 [`docs/README.md`](docs/README.md)。

在动任何代码之前，至少完成这三步：

1. 读 [`docs/AGENTS.md`](docs/AGENTS.md)（纪律、完成定义、验证命令矩阵、流程 checklist）。
2. 读 [`docs/00-conventions.md`](docs/00-conventions.md)（坐标、命名、Z 层、常量归属——唯一契约）。
3. 写任何 Bevy 代码前读 [`docs/10-bevy-0.20-api.md`](docs/10-bevy-0.20-api.md)（Bevy 0.17→0.20 有大量破坏性重命名，凭记忆写 API 必然出错）。

**不要在本文件里追加规范内容**——统一写入 `docs/`。
