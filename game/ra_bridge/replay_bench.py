import ast, json, re, sys, time, urllib.request

SRC = r"C:\Users\Dream\AppData\Local\Temp\claude\C--Users-Dream-Desktop-Z-Mortal\813ee6b7-3bf9-43aa-9cb6-87baec7ca430\tasks\b4mtd5dic.output"
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8472
pat = re.compile(r"^>>> (\S+) seat(\d) (\{.*\})$")

def post(kind, **kw):
    body = json.dumps({"kind": kind, **kw}).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}/", data=body)
    t0 = time.perf_counter()
    with urllib.request.urlopen(req) as r:
        dt = time.perf_counter() - t0
        return dt, json.loads(r.read())

lat, n_ev, cur_gid, inited = [], 0, None, set()
t_start = time.perf_counter()
with open(SRC, encoding="utf-8", errors="replace") as f:
    for line in f:
        m = pat.match(line.strip())
        if not m:
            continue
        gid, seat, ev = m.group(1), int(m.group(2)), ast.literal_eval(m.group(3))
        if gid != cur_gid:
            if cur_gid and cur_gid in inited:
                post("destroy", game_id=cur_gid)
            cur_gid, n_ev = gid, 0
        if gid not in inited:
            post("init", game_id=gid)
            inited.add(gid)
        n_ev += 1
        if n_ev > 4000:
            continue
        try:
            dt, resp = post("event", game_id=gid, seat=seat, line=ev)
        except urllib.error.HTTPError as he:
            err_body = he.read().decode()[:120]
            print("FALLBACK", gid, seat, ev.get("type"), err_body.splitlines()[-1][:80])
            continue
        except Exception as ex:
            print("ERR", gid, seat, ev.get("type"), ex)
            break
        if resp.get("action"):
            lat.append(dt)
        if len(lat) >= 300:
            break

lat.sort()
n = len(lat)
if n:
    print(f"decisions={n}  wall={time.perf_counter()-t_start:.1f}s")
    print(f"mean={sum(lat)/n*1000:.0f}ms  p50={lat[n//2]*1000:.0f}ms  p95={lat[int(n*.95)]*1000:.0f}ms  max={lat[-1]*1000:.0f}ms")
else:
    print("no decisions recorded")
