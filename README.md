# Z-Mortal

天凤（Tenhou）麻将 AI，基于 [Mortal](https://github.com/Equim-chan/Mortal) 架构训练。

## 模型

| 版本 | 文件 | 说明 |
|---|---|---|
| R4 (v0.1) | [`mortal_r4_v0.1.pth`](https://github.com/daniellelruizl36-cyber/Z-Mortal/releases/tag/v0.1) | stage-2 冠军权重（推理用精简存档） |

**模型卡**

- 架构：Mortal 兼容 ResNet（192 通道，约 1,080 万参数），参数版本 302（Mortal v3 obs 口径）
- 训练：人类牌谱监督预训练 + 自对弈强化学习两阶段；牌谱来源为天凤凤凰桌公开对局日志
- **强化学习奖励口径：祝仪桌现金 EV**（含红宝祝仪规则），而非天凤段位 pt——因此模型行为呈祝仪最优特征（例如手持红宝时显著偏好门清），这是奖励设计的结果，不是缺陷
- 权重格式：PyTorch checkpoint，只含策略网络 `mortal` 键（训练状态与超参数未包含在本发布中）
- 局限：针对天凤四麻东/半庄规则 + 祝仪现金 EV 奖励优化；在纯段位 pt 口径或其他规则集下不保证占优

**验证模型效果**：本发布已含完整推理权重与引擎，可自行复现对弈评测——

1. 从 release 下载 `mortal_r4_v0.1.pth`（可用 `SHA256SUMS.txt` 校验完整性）
2. 按上游 Mortal 文档构建本仓库附带的 `libriichi`（Rust）
3. 用 `mortal` 键载入 `Mortal` 网络；`mortal/` 目录内含对局与评测设施（`engine.py`、`one_vs_three.py`、`player.py`），可跑自对弈/三缺一半庄测评
4. 如需外部基准对比，请自行准备对手权重（如 Mortal 社区权重）；本发布不含任何对手模型

**加载方式**：权重兼容上游 Mortal 引擎，按上游文档构建 `libriichi`（本仓库已附带源码）后，
用 `mortal` 键载入 `Mortal` 网络即可对弈/评测。ckpt 结构：

```python
{'mortal': <policy state_dict>, 'steps': int, 'param_version': 302}
```

## 仓库内容

- `mortal/`：训练管线代码（监督预训练 + 在线 RL，含数据加载、对局、评测设施）
- `libriichi/`、`exe-wrapper/`：Mortal 的 Rust 核心（麻将对局引擎/批量推理），源码附带
- **不包含**：训练配置与超参数、实验记录

## 协议与声明

- 代码与权重均以 [AGPL-3.0](LICENSE) 发布（代码衍生自 Mortal，继承其协议）
- 本项目与天凤（天鳳）无隶属关系；牌谱版权归原作者所有
- 仅供研究与学习用途

---

**English summary**: Z-Mortal is a Tenhou mahjong AI trained on the Mortal architecture. This repo publishes the R4 stage-2 champion inference weights (Mortal-v3-obs compatible ResNet, ~10.8M params) together with the training pipeline code. The RL stage is optimized for zhuyi-table cash EV (a bonus-ruleset reward including red-five bonuses), not Tenhou ranking points — the model accordingly favors menzen play, especially with red fives in hand. Training hyperparameters and experiment logs are intentionally not included. Weights and engine are included, so gameplay evaluation can be reproduced locally. Code is AGPL-3.0, inherited from upstream Mortal. Not affiliated with Tenhou.
