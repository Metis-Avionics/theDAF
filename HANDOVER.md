# HANDOVER.md — theDAF

## Project: Data Access Factory (DAF)

### Current State (2026-10-25)

`main` @ `e41558f` = the #50 merge (seam-law audit + access-kit adoption —
content in `daf-core`'s generic module only). crates.io = all 9 `daf-*` at
**0.3.1** (published 2026-10-20, 11:40–11:42 UTC). This branch
(`chore/release-0.3.2`) prepares the **0.3.2 patch band**: no new public
surface (`typed_access_row` pre-existed on the published 0.3.1), no
signature or macro-grammar change, the `Default` bound relaxed; the 0.2
facade's deprecation posture is unchanged (still available, still annotated
deprecated — no removal in this band). All nine crates re-publish
band-wide per the workspace single-version model.

Publish recipe (the owner's tokened window; nothing on this lane publishes):

1. Verify the band: 9 lock rows + 16 intra-workspace reqs + 2 manifest rows
   (`Cargo.toml`, `theDAF.toml`) at 0.3.2.
2. Per crate, dry-run first: `cargo publish -p daf-core --dry-run` (and each
   of the nine), then the scripted order with the settled runner:
   `nohup bash scripts/publish_crates_io.sh <delay> > /tmp/crates-publish.log 2>&1 &`
   — order: `daf-core` → `daf-runtime` → `daf-repository` → `daf-algorithms`
   → `daf-cache` → `daf-messaging` → `daf-application` → `daf-http` →
   `daf-ffi`; skip-if-published idempotent; 429 backoff; 120 s settle
   between crates.
3. Registry verify: the API `max_stable_version` = 0.3.2 for all nine.

Downstream re-point (a downstream consumer pins the engines exact-band):
`daf-core "=0.3.1"` → `"=0.3.2"`; the `thesix =0.5.0` engine requirement
carries over unchanged (re-verified at the site); `themql-core` and the
`prv-*` rows untouched; no API migration — the generated access surface is
unchanged and the 0.2 facade remains available.

### Current State (2026-10-07)

**All 9 `daf-*` crates are live on crates.io at 0.2.0**, published from
`Metis-Avionics/theDAF` `main` @ `7ec1114` (metadata) on top of `369c0ec`
(#46: L0–L5 hierarchical cache integrated with the thesix retrofit).

Published order (leaf first): `daf-core` → `daf-runtime`, `daf-repository`,
`daf-algorithms` → `daf-cache` → `daf-messaging` → `daf-application` →
`daf-http` → `daf-ffi`. All show `max_version = 0.2.0` on the registry API
and carry `repository = https://github.com/Metis-Avionics/theDAF` (the
per-fork URL the 0.1.0 release advertised is gone from the manifests).

### What landed today (2026-10-07), all via PRs

| PR | Content | Disposition |
|---|---|---|
| #45 | thesix =0.4.0 Generation wrap, edition 2024, MSRV 1.99, theDAF.toml | MERGED (first merge ever to run green CI — see trigger fix below) |
| #44 | L0–L5 tiers + Moka prefix index + crates.io prep | MERGED (content integrated through #46) |
| #46 | #44 content integrated post-#45 + redis/sled compile fixes + parity-harness fix + PoT green | MERGED, all 7 checks green |
| #47 | 0.2.0 metadata (workspace single-sourced, version reqs) | MERGED, 8/9 green (graphify pre-existing) |
| #43 | dependabot httpx2 | MERGED (was inside #44's branch) |
| #30 | tier-aware cache + parity | CLOSED superseded by #24/#46 |

### The CI trigger bug (root cause of "never ran")

`.github/workflows/ci.yml` used `pull_requests:` (plural) — an invalid key,
so GitHub accepted the file but no PR event ever matched, and push runs
failed at 0s with "workflow file issue" on some days. Fixed directly on main
(`4e1abc7`, sanctioned direct push because a PR cannot validate its own
trigger). Every "pre-existing red" below was invisible before this fix.

### Known reds carried forward (NOT fixed here, maintainer rulings pending)

1. **graphify `directed_same_endpoint_collapsed_edges = 68 > threshold 30`**
   — measured identical on pre-#46 code and at `646c6eb+6`; the rise came in
   the red-team/PR24 chain, not from today's merges. Raising the threshold is
   a baseline raise and needs its own reviewed ruling; it is NOT raised here.
2. None. Everything else (lint, PoT py, PoT rust, rust-test, pytest,
   parity-differential, build) is green on `369c0ec` and `7ec1114`'s run.

### Findings worth carrying

- **Parity harness was state-divergent**: each test posted into a fresh
  python DAF but sent the python-issued rid to ONE shared rust proc whose
  repo never saw it (and rids are UUID vs ULID — never interchangeable).
  Fixed by mirrored per-runtime sequences; the rule is "the rid a side
  consumes must be a rid that side minted".
- **Rule-1 recursion found by a stack overflow, not by the static gate**:
  `as_u64()`'s "right-inverse" assert called `valid()`, which called
  `as_u64()` — mutual recursion under concurrent load. Rule 1 exists
  precisely for this; asserts must not recurse through constructors.
- **Tautological asserts are gate-gaming**: first attempts wrote
  `assert!(self.tier() == Tier::L2)`-style checks (including the
  self-recursive form). Replaced with ordering post-conditions that a Tier
  renumber would actually break.

### Next steps

1. TheThing side: vendor `daf-core` 0.2.0 from crates.io per living.toml's
   "ready to vendor" — this is the point of the 0.2.0 release.
2. Graphify threshold ruling (68 vs 30).
3. `scripts/publish_crates_io.sh`'s 0.1.0-era comments now describe the
   completed 0.1.0 release — update next touch.