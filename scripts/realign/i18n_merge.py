#!/usr/bin/env python3
"""Three-way merge of the i18n locale files, by key path instead of by line.

A locale file is a nested dict of translation strings, so a line-based merge
reports a conflict every time two unrelated keys land in the same neighbourhood.
Merging the trees instead makes the outcome decidable for every key on its own:

    key added by us, absent upstream        -> keep ours
    key added upstream, absent in ours      -> take theirs
    key deleted by us, still upstream       -> stays deleted (we purged it on purpose)
    key deleted upstream, unchanged by us   -> gone
    value changed on one side only          -> that side wins
    value changed on both sides, differently -> reported, ours wins

Ordering follows upstream so a later realign diffs cleanly against them, with
our own keys appended inside their parent object.

    i18n_merge.py BASE_DIR THEIRS_DIR OURS_DIR [--apply]
"""
import json
import sys
from pathlib import Path

MISSING = object()


def merge(base, theirs, ours, path, conflicts):
    if isinstance(theirs, dict) and isinstance(ours, dict):
        b = base if isinstance(base, dict) else {}
        out = {}
        for k in theirs:
            r = merge_key(b.get(k, MISSING), theirs[k], ours.get(k, MISSING),
                          path + [k], conflicts)
            if r is not MISSING:
                out[k] = r
        for k in ours:
            if k not in theirs:
                r = merge_key(b.get(k, MISSING), MISSING, ours[k], path + [k], conflicts)
                if r is not MISSING:
                    out[k] = r
        return out
    if ours == theirs or ours == base:
        return theirs
    if theirs == base:
        return ours
    conflicts.append(".".join(path))
    return ours


def merge_key(base, theirs, ours, path, conflicts):
    if theirs is MISSING and ours is MISSING:
        return MISSING
    if theirs is MISSING:
        # Upstream removed it. Ours survives only if we had changed it ourselves.
        return ours if base is MISSING or ours != base else MISSING
    if ours is MISSING:
        # We removed it. A deliberate purge outranks an upstream edit.
        return MISSING if base is not MISSING else theirs
    if base is MISSING:
        base = None
    return merge(base, theirs, ours, path, conflicts)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    apply_changes = "--apply" in sys.argv[1:]
    if len(args) != 3:
        sys.exit(__doc__)
    base_dir, theirs_dir, ours_dir = (Path(a) for a in args)

    rel = Path("src/i18n/locales")
    locales = sorted(p.name for p in (theirs_dir / rel).iterdir() if p.is_dir())
    total_conflicts = 0
    for loc in locales:
        f = rel / loc / "translation.json"
        theirs = json.loads((theirs_dir / f).read_text())
        base = json.loads((base_dir / f).read_text()) if (base_dir / f).exists() else {}
        if not (ours_dir / f).exists():
            merged, conflicts = theirs, []
        else:
            ours = json.loads((ours_dir / f).read_text())
            conflicts = []
            merged = merge(base, theirs, ours, [], conflicts)
        total_conflicts += len(conflicts)
        keys = count(merged)
        status = f"{len(conflicts)} conflicts" if conflicts else "clean"
        print(f"{loc:6} {keys:5} keys  {status}")
        for c in conflicts:
            print(f"       {c}")
        if apply_changes:
            out = ours_dir / f
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(json.dumps(merged, ensure_ascii=False, indent=2) + "\n")
    print(f"\n{len(locales)} locales, {total_conflicts} value conflicts (ours kept)")
    if apply_changes:
        print(f"written into {ours_dir / rel}")


def count(d):
    return sum(count(v) if isinstance(v, dict) else 1 for v in d.values())


if __name__ == "__main__":
    main()
