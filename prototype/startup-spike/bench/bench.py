#!/usr/bin/env python3
"""PROTOTYPE, throwaway: drives both spikes and summarises cold start and Sibling switch (acastaner/zimged#9).
usage: bench.py <image-dir> [runs]
Each cold-start run sleeps first so the CPU is idle at launch, like a real double-click."""
import json, os, statistics, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
QT = os.path.join(HERE, "../qt/build/zimged-spike-qt")
RS = os.path.join(HERE, "../rust/target/release/zimged-spike-rust")
imgdir, runs = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 15
first = os.path.join(imgdir, "IMG_0001.jpg")
x11 = {"WAYLAND_DISPLAY": None, "XDG_SESSION_TYPE": "x11"}
CONFIGS = {
    "qt / wayland": (QT, {}),
    "qt / wayland, no KDE platform theme": (QT, {"QT_QPA_PLATFORMTHEME": "none"}),
    "qt / x11 (XWayland)": (QT, {"QT_QPA_PLATFORM": "xcb", **x11}),
    "egui+glow / wayland": (RS, {"SPIKE_RENDERER": "glow"}),
    "egui+glow / x11 (XWayland)": (RS, {"SPIKE_RENDERER": "glow", **x11}),
    "egui+wgpu / wayland": (RS, {"SPIKE_RENDERER": "wgpu"}),
    "egui+wgpu / x11 (XWayland)": (RS, {"SPIKE_RENDERER": "wgpu", **x11}),
}

def run(binary, extra, args, timeout=60):
    env = dict(os.environ, LC_ALL="C.UTF-8")
    for k, v in extra.items():
        if v is None: env.pop(k, None)
        else: env[k] = v
    env["SPIKE_T0_NS"] = str(time.time_ns())
    try:
        out = subprocess.run([binary, *args], env=env, capture_output=True, text=True, timeout=timeout).stdout
    except subprocess.TimeoutExpired as e:  # e.g. the compositor stopped sending frame callbacks
        out = (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
    marks, switches = {}, []
    for line in out.splitlines():
        p = line.split()
        if len(p) >= 2 and p[0] == "switch_ms": switches.append(float(p[1]))
        elif len(p) >= 2:
            try: marks[p[0]] = float(p[1])
            except ValueError: pass
    return marks, switches

def pct(v, q):
    v = sorted(v); return v[min(len(v) - 1, int(round(q * (len(v) - 1))))]

results = {}
only = os.environ.get("BENCH_ONLY")
for name, (binary, extra) in CONFIGS.items():
    if only and name not in only.split(";"): continue
    cold = []
    for _ in range(runs):
        time.sleep(float(os.environ.get("BENCH_IDLE_S", "1.5")))
        m, _ = run(binary, {**extra, "SPIKE_EXIT": "1"}, [first])
        if "first_image_painted" in m: cold.append(m)
    nav = {}
    for label, spec in (("flip every 200 ms", "200,25"), ("hold the key (back-to-back)", "0,25")):
        time.sleep(1.5)
        _, sw = run(binary, {**extra, "SPIKE_NAV": spec}, [first], timeout=45)
        nav[label] = sw
    results[name] = {"cold": cold, "nav": nav}
    fp = [m["first_image_painted"] for m in cold] or [float("nan")]
    print(f"{name:40s} first image p50 {statistics.median(fp):6.1f} p90 {pct(fp, .9):6.1f} max {max(fp):6.1f} ms "
          + " | ".join(f"{k}: p50 {statistics.median(v):.1f} p90 {pct(v, .9):.1f} max {max(v):.1f}" for k, v in nav.items() if v) + ("" if all(nav.values()) else " | nav stalled"), flush=True)

with open(os.path.join(HERE, os.environ.get("BENCH_OUT", "results-linux.json")), "w") as f:
    json.dump(results, f, indent=1)
