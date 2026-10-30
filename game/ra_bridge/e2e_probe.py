import json, sys, time, urllib.request
port = sys.argv[1] if len(sys.argv) > 1 else "8471"
def post(kind, **kw):
    body = json.dumps({"kind": kind, **kw}).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}/", data=body)
    with urllib.request.urlopen(req) as r: return json.loads(r.read())
print(post("init", game_id="E2E"))
sk = {"bakaze":"E","dora_marker":"W","honba":0,"kyoku":1,"kyotaku":0,"oya":0,
      "scores":[25000]*4,
      "tehais":[["1m","2m","3m","4m","5m","6m","7m","8m","9m","1p","2p","3p","4p"]]*4,
      "type":"start_kyoku"}
for s in range(4): print(s, post("event", game_id="E2E", seat=s, line=sk).get("action"))
ts = {"actor":0,"pai":"5s","type":"tsumo"}
t0=time.perf_counter(); r=post("event", game_id="E2E", seat=0, line=ts)
print("tsumo->", r.get("action"), " %.0fms" % ((time.perf_counter()-t0)*1000))
print(post("destroy", game_id="E2E"))
