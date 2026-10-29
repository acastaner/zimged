#!/usr/bin/env python3
"""tools/bench: zimged's performance budgets (spec §1.2), descended from the cold-start spike's
bench.py (branch prototype/startup-spike, acastaner/zimged#9).

Not part of the gate: it depends on the hardware and the power profile. Run it by hand on the
reference machine (i5-12600KF, balanced power profile, Wayland) and put the numbers in the release
notes.

  tools/bench/bench.py fixture DIR [--count N]
      Write N (default 1) generated 12 MP (4000x3000) JPEGs, IMG_0001.jpg..., into DIR. Needs Pillow.
      Keep them out of the repository.

  tools/bench/bench.py cold IMAGE [--runs 15] [--idle 1.5] [--x11] [--binary PATH] [--out FILE]
      Cold start (§1.2): launch zimged on IMAGE `runs` times, each after `idle` seconds of idle,
      and time launch -> first image painted. Wayland unless --x11 (XWayland).

The clock starts just before exec (ZIMGED_BENCH_T0_NS) and stops when zimged builds the frame that
draws the first image (`first_image_painted`), which eframe paints and hands to the compositor
straight after; it appears on screen at the next vblank. ZIMGED_BENCH_EXIT=1 makes zimged close
right after. Other marks: main, renderer_ready (window + GL context created), decoded.

Sibling switching and the 10,000-file folder are added with the Sibling navigation ticket (#15).
"""
import argparse, json, os, platform, statistics, subprocess, sys, time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DEFAULT_BINARY = os.path.join(ROOT, "target", "release", "zimged")
BUDGET_COLD_P90_MS = 150.0


def pct(values, q):
    v = sorted(values)
    return v[min(len(v) - 1, int(round(q * (len(v) - 1))))]


def power_profile():
    try:
        return subprocess.run(["powerprofilesctl", "get"], capture_output=True, text=True, timeout=5).stdout.strip() or "unknown"
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def cpu_model():
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def launch(binary, image, x11, timeout=30):
    env = dict(os.environ, LC_ALL="C.UTF-8", ZIMGED_BENCH_EXIT="1")
    if x11:
        env.pop("WAYLAND_DISPLAY", None)
        env["XDG_SESSION_TYPE"] = "x11"
    env["ZIMGED_BENCH_T0_NS"] = str(time.time_ns())
    try:
        out = subprocess.run([binary, image], env=env, capture_output=True, text=True, timeout=timeout).stdout
    except subprocess.TimeoutExpired as e:  # e.g. the compositor stopped sending frame callbacks
        out = e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
    marks = {}
    for line in out.splitlines():
        p = line.split()
        if len(p) == 2:
            try:
                marks[p[0]] = float(p[1])
            except ValueError:
                pass
    return marks


def cmd_cold(args):
    if not os.access(args.binary, os.X_OK):
        sys.exit(f"no zimged binary at {args.binary}: run `cargo build --release` first")
    session = "x11 (XWayland)" if args.x11 else ("wayland" if os.environ.get("WAYLAND_DISPLAY") else os.environ.get("XDG_SESSION_TYPE", "?"))
    profile = power_profile()
    print(f"cold start: {args.runs} runs, {args.idle}s idle, {session}, power profile {profile}, {cpu_model()}", flush=True)
    runs, failed = [], 0
    for i in range(args.runs):
        time.sleep(args.idle)
        m = launch(args.binary, args.image, args.x11)
        if "first_image_painted" in m:
            runs.append(m)
            print(f"  run {i + 1:2d}: {m['first_image_painted']:6.1f} ms", flush=True)
        else:
            failed += 1
            print(f"  run {i + 1:2d}: no first_image_painted mark ({m})", flush=True)
    if not runs:
        sys.exit("no run painted an image")
    fp = [m["first_image_painted"] for m in runs]
    phases = {k: statistics.median(m[k] for m in runs if k in m) for k in ("main", "renderer_ready", "decoded", "first_image_painted") if any(k in m for m in runs)}
    p90 = pct(fp, 0.9)
    summary = {
        "p50": statistics.median(fp), "p90": p90, "max": max(fp), "min": min(fp),
        "phases_median": phases, "failed_runs": failed,
        "budget_p90_ms": BUDGET_COLD_P90_MS, "within_budget": p90 <= BUDGET_COLD_P90_MS,
    }
    print(f"first image painted: p50 {summary['p50']:.1f}  p90 {p90:.1f}  max {summary['max']:.1f} ms "
          f"(budget p90 <= {BUDGET_COLD_P90_MS:.0f}: {'met' if summary['within_budget'] else 'MISSED'})")
    print("phases (median ms from launch): " + " · ".join(f"{k} {v:.1f}" for k, v in phases.items()))
    if args.out:
        with open(args.out, "w") as f:
            json.dump({
                "bench": "cold", "image": os.path.abspath(args.image), "binary": os.path.abspath(args.binary),
                "session": session, "power_profile": profile, "cpu": cpu_model(),
                "runs": args.runs, "idle_s": args.idle, "summary": summary, "marks": runs,
            }, f, indent=1)
        print(f"results written to {args.out}")


def cmd_fixture(args):
    try:
        from PIL import Image, ImageDraw, ImageFilter
    except ImportError:
        sys.exit("generating fixtures needs Pillow (python3 -m pip install pillow)")
    import random
    os.makedirs(args.dir, exist_ok=True)
    w, h = 4000, 3000
    for n in range(1, args.count + 1):
        rnd = random.Random(n)
        # Photo-like content: smooth colour fields, shapes and fine grain, so the JPEG has about the
        # entropy (3-5 MB at quality 92) of a 12 MP camera image rather than compressing to nothing.
        base = Image.new("RGB", (w // 8, h // 8))
        draw = ImageDraw.Draw(base)
        for _ in range(60):
            x, y = rnd.randrange(w // 8), rnd.randrange(h // 8)
            r = rnd.randrange(20, 200)
            draw.ellipse([x - r, y - r, x + r, y + r], fill=tuple(rnd.randrange(256) for _ in range(3)))
        img = base.filter(ImageFilter.GaussianBlur(12)).resize((w, h), Image.BICUBIC)
        grain = Image.merge("RGB", [Image.effect_noise((w, h), 40 + 10 * c) for c in range(3)])
        img = Image.blend(img, grain, 0.18)
        path = os.path.join(args.dir, f"IMG_{n:04d}.jpg")
        img.save(path, quality=92)
        print(f"{path}: {w}x{h}, {os.path.getsize(path) / 1e6:.1f} MB")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("cold", help="cold start: launch to first image painted")
    c.add_argument("image")
    c.add_argument("--runs", type=int, default=15)
    c.add_argument("--idle", type=float, default=1.5)
    c.add_argument("--x11", action="store_true", help="run under XWayland instead of Wayland")
    c.add_argument("--binary", default=DEFAULT_BINARY)
    c.add_argument("--out", help="write the marks and summary as JSON")
    c.set_defaults(func=cmd_cold)
    f = sub.add_parser("fixture", help="generate 12 MP JPEGs")
    f.add_argument("dir")
    f.add_argument("--count", type=int, default=1)
    f.set_defaults(func=cmd_fixture)
    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
