"""Write THIRD-PARTY-NOTICES.txt for the release: every crate compiled
into claim-jumpers.exe (the normal dependencies of cj-game on Windows),
with its licence and the licence files it ships.

Run from the native folder: python tools/third_party.py OUT_FILE
"""
import json
import os
import subprocess
import sys


def main():
    out = sys.argv[1]
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"],
        text=True,
    ))
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "cj-game")
    # Walk normal (not build or dev) dependencies only: what ships in the exe.
    seen, todo = set(), [root]
    while todo:
        nid = todo.pop()
        if nid in seen:
            continue
        seen.add(nid)
        for d in nodes[nid]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    ours = {"cj-game", "cj-sim"}
    crates = sorted((pkgs[i] for i in seen if pkgs[i]["name"] not in ours), key=lambda p: (p["name"], p["version"]))
    lines = [
        "Claim Jumpers includes the following open-source Rust crates.",
        "Each is listed with its licence; the licence texts each crate ships follow.",
        "",
    ]
    for p in crates:
        lines.append(f"{p['name']} {p['version']}: {p.get('license') or 'see licence file'}"
                     + (f" ({p['repository']})" if p.get("repository") else ""))
    for p in crates:
        d = os.path.dirname(p["manifest_path"])
        files = sorted(f for f in os.listdir(d)
                       if f.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")) and os.path.isfile(os.path.join(d, f)))
        if not files:
            continue
        lines += ["", "=" * 72, f"{p['name']} {p['version']}", "=" * 72]
        for f in files:
            with open(os.path.join(d, f), encoding="utf-8", errors="replace") as fh:
                lines += ["", f"--- {f} ---", "", fh.read().rstrip()]
    with open(out, "w", encoding="utf-8", newline="\r\n") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"{len(crates)} crates -> {out}")


if __name__ == "__main__":
    main()
