# Z-Mortal 游戏客户端

开箱即用的天凤麻将 AI 对打游戏：本目录包含游戏程序的全部源码与打包脚本。AI 为本仓库训练的 R4 冠军模型（Release [v0.1](https://github.com/daniellelruizl36-cyber/Z-Mortal/releases/tag/v0.1)）。

## 下载安装

到 Release [`game-v0.1`](https://github.com/daniellelruizl36-cyber/Z-Mortal/releases/tag/game-v0.1) 下载 `Z-Mortal-Setup.exe`（Windows x64，免管理员权限），安装后双击桌面图标即可。程序会在本机启动游戏服务器（默认端口 4002）与 AI 引擎（127.0.0.1:8471），自动打开浏览器进入大厅。

- 支持人机对打（1 人 + 3 个 R4 AI）与 4 个 AI 自对旁观
- 内置两套规则：SRL 联盟规（日常）与广州祝仪规则（现金 EV 口径，与本模型训练奖励一致）

## 目录结构

| 路径 | 说明 |
|---|---|
| `riichi-advanced/` | 游戏程序本体（[Riichi Advanced](https://github.com/EpicOrange/riichi_advanced) 的修改版，Elixir + Phoenix） |
| `ra_bridge/` | R4 AI 接入桥：`r4_service.py`（HTTP 服务，加载 Mortal 引擎对局）与开发/诊断脚本 |
| `installer/` | 打包脚本：`setup.iss`（Inno Setup）、`start.bat`/`stop.bat`/`Launch.vbs`（安装包内启动链路） |

### 对 Riichi Advanced 的主要修改

- `lib/riichi_advanced/game/r4_player.ex`：AI 玩家 GenServer，通过 HTTP 把牌局事件流转发给 r4_service，并把 AI 决策（切牌/立直/副露）翻译回游戏动作
- `lib/riichi_advanced/r4_demo.ex`：人机模式编排（人入座后 AI 自动补满空位）
- `config/runtime.exs`、`mix.exs`：prod release 配置（端口、`validate_compile_env: false`、`releases` 声明等）
- 内置祝仪规则集 `zhuyi_guangzhou` 与 SRL 联盟规 `.majs` 文件

### AI 桥接协议

`r4_service.py` 监听 `127.0.0.1:8471`，接收 mjai 协议事件流（`init`/`start_game`/`start_kyoku`/`event`），由 Mortal 引擎（`libriichi` 批量推理）决策并返回动作。R4 模型权重为完整训练 checkpoint（含全部网络参数），由安装包内置。

## 自行构建安装包

1. 准备 Erlang/OTP + Elixir（与上游 Riichi Advanced 要求一致）、Python 3.12、Inno Setup 6
2. 在 `riichi-advanced/` 内：`mix deps.get` 后 `MIX_ENV=prod mix release`（注意 `mix.exs` 中 `releases` 必须写在 `project/0` 关键字里）
3. 准备 AI 运行环境：嵌入式 Python 3.12 + torch（CPU 版）+ `mortal/` 三件套（`libriichi.pyd`、`model.py`、`engine.py`）+ `models/r4_gen2.pth` 权重
4. 按 `installer/start.bat` 的目录布局组装，用 Inno Setup 编译 `installer/setup.iss`

## 协议

- **AGPL-3.0**：本目录整体（含 Riichi Advanced 修改版与桥接代码）。基于 [EpicOrange/riichi_advanced](https://github.com/EpicOrange/riichi_advanced)（AGPL-3.0）修改，对应源码即 `riichi-advanced/`
- **AGPL-3.0**：AI 引擎 [Equim-chan/Mortal](https://github.com/Equim-chan/Mortal) 及 `libriichi` Rust 核心，源码在仓库根目录 `mortal/`、`libriichi/`
- 模型权重随安装包发布，可自由用于推理、研究与再训练；再分发修改版须遵守 AGPL
