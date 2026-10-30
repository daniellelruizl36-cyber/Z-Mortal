# r4_service 原型：验证 libriichi.mjai.Bot + R4 champion 从 Python 可调通。
# 跑法: D:\\tools\\Anaconda\\envs\\mortal\\python.exe proto_bot.py
import sys, json, os
sys.path.insert(0, r"C:\Users\Dream\Desktop\Z-Mortal\Mortal\mortal")
os.chdir(r"C:\Users\Dream\Desktop\Z-Mortal\Mortal\mortal")

import torch
from model import Brain, DQN
from engine import MortalEngine
from libriichi.mjai import Bot

PTH = r"C:\Users\Dream\Desktop\Z-Mortal\zmortal_local\models\archive\mortal_stage2_r4_gen2_champion_20260930.pth"

def load_engine(pth):
    state = torch.load(pth, weights_only=True, map_location="cpu")
    cfg = state["config"]
    version = cfg["control"].get("version", 1)
    print(f"checkpoint: version={version} resnet={cfg['resnet']}")
    mortal = Brain(version=version, **cfg["resnet"]).eval()
    dqn = DQN(version=version).eval()
    mortal.load_state_dict(state["mortal"])
    dqn.load_state_dict(state["current_dqn"])
    dev = torch.device("cuda:0" if torch.cuda.is_available() else "cpu")
    print(f"device: {dev}")
    return MortalEngine(mortal, dqn, is_oracle=False, version=version,
                        device=dev, enable_amp=True,
                        enable_rule_based_agari_guard=True, name="r4proto")

engine = load_engine(PTH)
bot = Bot(engine, 0)

def feed(line, can_act=True):
    ev = json.dumps(line) if not isinstance(line, str) else line
    r = bot.react(ev, can_act=can_act)
    print(f"  <- {ev[:90]}")
    if r is not None:
        print(f"  => ACTION: {r}")
    return r

feed({"type": "start_game", "names": ["R4A", "R4B", "R4C", "R4D"]})
feed({"type": "start_kyoku", "bakaze": "E", "dora_marker": "3p", "kyoku": 1,
      "honba": 0, "kyotaku": 0, "oya": 0,
      "scores": [25000, 25000, 25000, 25000],
      "tehais": [
          ["1m", "2m", "3m", "4m", "5m", "6m", "7p", "8p", "9p", "3s", "4s", "5sr", "9s"],
          ["1p", "1p", "2p", "3p", "4p", "5p", "6p", "7p", "8p", "9p", "9p", "9p", "1s"],
          ["1m", "4m", "7m", "2p", "5p", "8p", "2s", "5s", "8s", "E", "S", "W", "N"],
          ["9m", "8m", "7m", "9p", "8p", "7p", "9s", "8s", "7s", "P", "F", "C", "E"]]})
feed({"type": "tsumo", "actor": 0, "pai": "2p"})   # 自家摸牌 → 应返回打牌
feed({"type": "dahai", "actor": 0, "pai": "9s", "tsumogiri": False}, can_act=False)
feed({"type": "tsumo", "actor": 1, "pai": "E"}, can_act=False)
feed({"type": "dahai", "actor": 1, "pai": "1s", "tsumogiri": False}, can_act=False)
feed({"type": "tsumo", "actor": 2, "pai": "P"}, can_act=False)
feed({"type": "dahai", "actor": 2, "pai": "P", "tsumogiri": True}, can_act=False)
feed({"type": "tsumo", "actor": 3, "pai": "W"}, can_act=False)
feed({"type": "dahai", "actor": 3, "pai": "W", "tsumogiri": True}, can_act=False)
feed({"type": "tsumo", "actor": 0, "pai": "E"})   # 又该我 → 返回动作
feed({"type": "reach", "actor": 0}, can_act=False)  # 假装有人立直（测试事件容忍）
feed({"type": "end_kyoku"})
feed({"type": "end_game"})
print("PROTO OK")
