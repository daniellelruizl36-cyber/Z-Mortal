# R4 bridge service: Riichi Advanced (Elixir) -> R4 gen2 (libriichi.mjai.Bot wrapper)
# Zero-dependency HTTP (stdlib only). One MortalEngine shared by all seats/bots.
#
#   POST /init    {"game_id": "R4DEMO"}                      -> create 4 bots
#   POST /event   {"game_id": "R4DEMO", "seat": 0, "line": {<mjai event>}}
#                                                            -> {"action": {<mjai action>} | null}
#   POST /destroy {"game_id": "R4DEMO"}                      -> drop bots
#
# All bots receive every event (can_act decided by libriichi internals);
# react() only returns an action when that seat can actually act.
# Raising propagate as HTTP 500 with the error text (Elixir side falls back).
import sys, os, json, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# Layout-relative defaults (overridable via env):
#   <root>/ai/mortal          -> Mortal python package
#   <root>/models/r4_gen2.pth -> champion weights
# where <root> is the folder containing this script's parent (ai/..).
# Falls back to the local dev checkout layout when the packaged layout
# isn't present.
_AI_DIR = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(_AI_DIR)
_PACKAGED_MORTAL = os.path.join(_ROOT, "ai", "mortal")
if os.environ.get("R4_MORTAL_DIR"):
    MORTAL_DIR = os.environ["R4_MORTAL_DIR"]
elif os.path.isdir(_PACKAGED_MORTAL):
    MORTAL_DIR = _PACKAGED_MORTAL
else:
    MORTAL_DIR = r"C:\Users\Dream\Desktop\Z-Mortal\Mortal\mortal"
sys.path.insert(0, MORTAL_DIR)
os.chdir(MORTAL_DIR)

import torch
from model import Brain, DQN
from engine import MortalEngine
from libriichi.mjai import Bot

_PACKAGED_PTH = os.path.join(_ROOT, "models", "r4_gen2.pth")
if os.environ.get("R4_PTH"):
    PTH = os.environ["R4_PTH"]
elif os.path.isfile(_PACKAGED_PTH):
    PTH = _PACKAGED_PTH
else:
    PTH = r"C:\Users\Dream\Desktop\Z-Mortal\zmortal_local\models\archive\mortal_stage2_r4_gen2_champion_20260930.pth"
PORT = int(os.environ.get("R4_PORT", "8471"))

def load_engine(pth):
    state = torch.load(pth, weights_only=True, map_location="cpu")
    cfg = state["config"]
    version = cfg["control"].get("version", 1)
    mortal = Brain(version=version, **cfg["resnet"]).eval()
    dqn = DQN(version=version).eval()
    mortal.load_state_dict(state["mortal"])
    dqn.load_state_dict(state["current_dqn"])
    dev = torch.device("cuda:0" if torch.cuda.is_available() else "cpu")
    eng = MortalEngine(mortal, dqn, is_oracle=False, version=version, device=dev,
                       enable_amp=True, enable_rule_based_agari_guard=True, name="r4")
    # warm-up forward so the first real decision isn't slow
    eng.react_batch([__import__("numpy").zeros((934, 34), dtype="float32")],
                    [__import__("numpy").array([True] * 34 + [False] * 12)],
                    None)
    return eng

print(f"loading {PTH} ...", flush=True)
ENGINE = load_engine(PTH)
print("engine ready", flush=True)

LOCK = threading.Lock()
GAMES = {}  # game_id -> [Bot x4]

def handle(req):
    global GAMES
    kind = req["kind"]
    if kind == "init":
        gid = req["game_id"]
        with LOCK:
            if gid not in GAMES:
                GAMES[gid] = [Bot(ENGINE, s) for s in range(4)]
        return {"ok": True}
    if kind == "destroy":
        with LOCK:
            GAMES.pop(req["game_id"], None)
        return {"ok": True}
    if kind == "event":
        gid, seat, line = req["game_id"], req["seat"], req["line"]
        if os.environ.get("R4_DEBUG"):
            print(f">>> {gid[:8]} seat{seat} {line}", flush=True)
        with LOCK:
            bots = GAMES[gid]
            r = bots[seat].react(json.dumps(line))
        if os.environ.get("R4_DEBUG") and r:
            print(f"<<< {gid[:8]} seat{seat} {r}", flush=True)
        return {"action": json.loads(r) if r else None}
    raise ValueError(f"unknown kind {kind}")

class H(BaseHTTPRequestHandler):
    def do_POST(self):
        try:
            n = int(self.headers["Content-Length"])
            req = json.loads(self.rfile.read(n))
            body = json.dumps(handle(req)).encode()
            self.send_response(200)
        except Exception as ex:
            body = json.dumps({"error": str(ex)}).encode()
            self.send_response(500)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass

if __name__ == "__main__":
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), H)
    print(f"serving on 127.0.0.1:{PORT}", flush=True)
    srv.serve_forever()
