# Plan: PR24 Adversarial Pass 3 — Spec and Agent Prompt

## Context

PR24 adversarial review passes 1 and 2 are complete (posted as PR comments in Session 020).  
This plan creates the specification and agent-prompt artifacts for **Pass 3**, which shifts
the review focus from correctness bugs to **concurrency-preservation as an architectural invariant**.

**Current state of specialized primitives:**
- `Cachelito`: Not yet adopted. The architectural translation plan (`1786810915934`) specifies Cachelito as the planned L1 backend (`cachelito::Cache<String, Arc<dyn Any + Send + Sync>>`), but the current L1 is `MemoryCache` using `Arc<RwLock<MemoryCacheInner>>`.
- `DashMap`: Not present anywhere in the codebase, not referenced in architectural plans.

**Pass 3's actual question:** Does the current `Cache` trait and `HierarchicalCache` composition preserve the concurrency characteristics required for future adoption of Cachelito (L1) and any concurrent map primitive, without requiring architectural changes that would "flatten" the hierarchy?

The agent must **not** rewrite existing primitives, and must **not** recommend adopting Cachelito/DashMap in this pass. The review is forensic: determine whether the abstraction is ready.

## What to create

Create the following two files in `.kilo/plans/`. Both files are complete specifications
provided below; the executing agent must write them verbatim.

---

### File 1: `.kilo/plans/PR24_ADVERSARIAL_PASS3.toml`

```toml
[spec]
id = "theDAF.pr24.adversarial.pass3"
version = "3.0.0"
title = "PR24 Adversarial Review Pass 3"
repository = "Metis-Avionics/theDAF"
target = "pull_request:24"

objective = """
Perform a systems-level adversarial review of PR24 with particular emphasis on
concurrency architecture, cache hot-path behavior, compatibility with planned
specialized cache primitives (Cachelito), generation-based coherence, and
interactions between concurrent reads and invalidation.

The review must determine whether the current Cache trait and HierarchicalCache
composition are compatible with future adoption of Cachelito (L1) without
requiring architectural changes that would flatten the hierarchy.

The review is not a refactoring exercise. Do not rewrite cache primitives.
Do not recommend adopting Cachelito or DashMap in this pass.
Identify architectural violations, race conditions, contention hazards,
semantic regressions, and missing evidence.
"""

[constraints]
no_primitive_rewrite = true
no_speculative_optimization = true
no_generic_lock_replacement = true
no_behavior_change_without_evidence = true
preserve_existing_cache_primitives = true
preserve_public_api_unless_required = true
review_source_and_living_docs = true
review_full_pr_history = true
review_current_diff = true

[required_context]
pull_request = 24
branch_head = "current"
repository = "Metis-Avionics/theDAF"

[architectural_intent]
cache_model = "hierarchical"
tier_order = ["L1", "L2", "L3", "L4"]

# Current L1: MemoryCache (Arc<RwLock<MemoryCacheInner>>)
# Planned L1:  Cachelito (per-key concurrent reads, process-local)
# L2:          MokaCache (moka::future::Cache, async-native)
# L3:          RedisCache (stub behind feature flag)
# L4:          PostgresCache (stub behind feature flag)

read_path = """
L1 -> L2 -> L3 -> L4
"""

write_path = """
repository mutation
-> generation advancement
-> cache invalidation
"""

authoritative_state = "repository + generation"
cache_role = "advisory"

l1_intent = "read-heavy hot-path cache (currently MemoryCache; Cachelito planned per architectural translation)"
l2_intent = "secondary cache"
l3_intent = "distributed/remote cache"
l4_intent = "persistent fallback"

specialized_primitives = [
    "Cachelito",
    "DashMap"
]

# Note: Cachelito is specified as the planned L1 backend in the architectural
# translation plan but is not yet adopted. DashMap is not referenced anywhere
# in the codebase or architectural plans. The review treats these as the
# intended specialized primitives and evaluates whether the current abstraction
# is compatible with their concurrency models.

[concurrency_requirements]
read_heavy_paths_must_remain_concurrent = true
l1_reads_should_not_be_globally_serialized = true
cache_hierarchy_must_not_hide_contention = true
promotion_must_not_introduce_global_locking = true
invalidation_must_not_corrupt_concurrent_reads = true
generation_reads_must_be_safe_under_concurrency = true

[concurrency_requirements.dashmap]
requirement = """
DashMap is not currently in the codebase. Audit whether the current abstraction
would preserve DashMap's sharded concurrent map semantics IF it were added.
Flag any pattern that would force external locking around DashMap or serialize
access at a coarser granularity than DashMap's shards.
"""

[concurrency_requirements.cachelito]
requirement = """
Cachelito is the planned L1 backend per the architectural translation plan but
is not yet adopted. Audit whether the current Cache trait and HierarchicalCache
composition would preserve Cachelito's per-key concurrent read semantics IF it
were adopted as L1. Do not recommend adopting Cachelito in this pass.
Do not propose replacing Cachelito with a generic primitive when it IS adopted.
"""

[forbidden_remediations]
# These apply to FUTURE adoption of Cachelito/DashMap, not current code.
# The current L1 (MemoryCache) uses RwLock<HashMap> as an interim implementation.
# Do not recommend changing MemoryCache in this pass; that is out of scope.
items = [
    "Replace Cachelito with HashMap",
    "Replace Cachelito with RwLock<HashMap>",
    "Replace Cachelito with Mutex<HashMap>",
    "Replace DashMap with HashMap behind a global lock",
    "Introduce a global mutex around HierarchicalCache",
    "Rewrite cache primitives without benchmark evidence",
    "Treat concurrency regressions as acceptable implementation details",
    "Suppress race or contention findings because unit tests pass"
]

[review_axes]

[[review_axes.item]]
id = "CONCURRENCY-001"
name = "L1 read concurrency"
severity = "P0/P1"
question = """
Can independent L1 reads proceed concurrently, or has the abstraction introduced
a shared lock, serialized executor path, or other bottleneck?
"""

[[review_axes.item]]
id = "CONCURRENCY-002"
name = "L1 primitive compatibility"
severity = "P1"
question = """
If Cachelito were adopted as L1, would HierarchicalCache preserve its
per-key concurrent read semantics, or would the hierarchy add synchronization
that defeats Cachelito's concurrency model?
"""

[[review_axes.item]]
id = "CONCURRENCY-003"
name = "DashMap compatibility"
severity = "P1"
question = """
If a DashMap-like concurrent map were added to any tier, would callers or
HierarchicalCache introduce coarse-grained synchronization around it?
Flag lock amplification or serialization that would defeat DashMap's sharded
concurrency model. (DashMap is not currently in the codebase; this is a
forward-looking compatibility audit.)
"""

[[review_axes.item]]
id = "CONCURRENCY-004"
name = "Lock amplification"
severity = "P0/P1"
question = """
Does a single high-level cache operation acquire multiple locks or serialize
operations across otherwise independent keys or tiers?
"""

[[review_axes.item]]
id = "CONCURRENCY-005"
name = "Promotion contention"
severity = "P1"
question = """
Does L2/L3/L4 -> L1 promotion introduce contention that defeats the purpose of
the concurrent L1 primitive?
"""

[[review_axes.item]]
id = "CONCURRENCY-006"
name = "Concurrent invalidation"
severity = "P0"
question = """
Can invalidation race with reads or promotion in a way that returns stale data,
resurrects invalid entries, or creates inconsistent generation state?
"""

[[review_axes.item]]
id = "CONCURRENCY-007"
name = "Generation coherence"
severity = "P0"
question = """
Can concurrent mutation and cache access observe an old generation after the
repository mutation has committed?
"""

[[review_axes.item]]
id = "CONCURRENCY-008"
name = "Cross-tier semantics"
severity = "P1"
question = """
Does each cache tier preserve its intended concurrency and failure semantics,
or does the hierarchy impose one synchronization model on every backend?
"""

[[review_axes.item]]
id = "CONCURRENCY-009"
name = "Async contention"
severity = "P1"
question = """
Are blocking locks, synchronous critical sections, or expensive operations
performed inside async cache paths?
"""

[[review_axes.item]]
id = "CONCURRENCY-010"
name = "Clone/allocation pressure"
severity = "P2"
question = """
Does cache promotion or result handling introduce unnecessary cloning,
allocation, serialization, or Arc churn on the read hot path?
"""

[[review_axes.item]]
id = "CONCURRENCY-011"
name = "Async boundary contention"
severity = "P1"
question = """
Does the Cache trait's async boundary force serialization that a synchronous
concurrent primitive (like Cachelito) would not incur? Specifically: does
HierarchicalCache::get() hold locks or await points that would serialize
independent L1 reads when the underlying primitive is concurrent?
"""

[[review_axes.item]]
id = "CONCURRENCY-012"
name = "Evidence"
severity = "P1"
question = """
Are claims about concurrency backed by implementation evidence, tests,
benchmarks, documentation, or source-level guarantees?
"""

[coherence]
model = "generation_based"
required_invariant = """
Once a repository mutation commits, every subsequently accepted cache value
must correspond to the current generation.
"""

required_properties = [
    "generation advancement must not depend on successful cache deletion",
    "cache invalidation failure must not make committed repository state appear uncommitted",
    "concurrent readers must reject values from obsolete generations",
    "promotion must not bypass generation validation",
    "partial tier invalidation must not permit stale values to be accepted"
]

[read_path_analysis]
required = true

steps = [
    "trace L1 lookup",
    "trace L1 miss",
    "trace L2 lookup",
    "trace L2 miss",
    "trace L3 lookup",
    "trace L3 miss",
    "trace L4 fallback",
    "trace promotion",
    "trace generation validation",
    "trace returned value"
]

for_each_step = [
    "identify synchronization",
    "identify allocation",
    "identify clone/copy",
    "identify await",
    "identify failure mode",
    "identify contention scope"
]

[invalidation_analysis]
required = true

scenarios = [
    "read during invalidation",
    "read immediately after repository mutation",
    "promotion during invalidation",
    "concurrent invalidation of different keys",
    "concurrent invalidation of same key",
    "prefix invalidation during reads",
    "full cache shake during reads",
    "Moka degraded invalidation",
    "L1 invalidation failure",
    "L2 invalidation failure",
    "L3 invalidation failure",
    "L4 invalidation failure"
]

[adversarial_scenarios]
required = true

[[adversarial_scenarios.case]]
id = "A1"
name = "hot-key read storm"
description = """
Simulate many concurrent readers targeting the same hot key. Determine whether
the hierarchy serializes access or preserves concurrent reads.
"""

[[adversarial_scenarios.case]]
id = "A2"
name = "many-key read storm"
description = """
Simulate many concurrent readers targeting independent keys. Determine whether
the implementation accidentally creates global contention.
"""

[[adversarial_scenarios.case]]
id = "A3"
name = "read/write collision"
description = """
Concurrent reads occur while a mutation invalidates the same key.
Determine whether a stale value can escape.
"""

[[adversarial_scenarios.case]]
id = "A4"
name = "promotion race"
description = """
Two or more readers miss L1 and promote the same lower-tier value concurrently.
Determine whether duplicate work, contention, or stale resurrection occurs.
"""

[[adversarial_scenarios.case]]
id = "A5"
name = "mutation storm"
description = """
Concurrent mutations invalidate overlapping cache regions. Determine whether
generation and invalidation ordering remains coherent.
"""

[[adversarial_scenarios.case]]
id = "A6"
name = "Moka degraded backend"
description = """
Moka performs full invalidation for a prefix operation and returns an error.
Determine whether repository state, generation state, and lower cache tiers
remain coherent.
"""

[[adversarial_scenarios.case]]
id = "A7"
name = "ABA generation"
description = """
Attempt to construct a sequence in which stale cache state becomes
indistinguishable from current state because generation advancement is skipped,
reordered, or reused.
"""

[[adversarial_scenarios.case]]
id = "A8"
name = "lock amplification"
description = """
Trace whether one cache operation causes unrelated keys, requests, or tiers to
share synchronization unnecessarily.
"""

[[adversarial_scenarios.case]]
id = "A9"
name = "primitive substitution"
description = """
Search for any implementation pattern that would force a future specialized
concurrent primitive (Cachelito, DashMap) to be replaced with a weaker generic
synchronization structure (HashMap, RwLock<HashMap>, Mutex<HashMap>, global lock).

Specifically: does HierarchicalCache or any caller add a synchronization layer
that would make Cachelito's per-key concurrency indistinguishable from a global
lock?
"""

[[adversarial_scenarios.case]]
id = "A10"
name = "async blocking"
description = """
Search for synchronous locks or blocking work executed across async boundaries.
"""

[reporting]
format = "structured"

[reporting.required_sections]
sections = [
    "executive_verdict",
    "architecture_model",
    "concurrency_findings",
    "coherence_findings",
    "primitive_compatibility",
    "read_path_analysis",
    "invalidation_analysis",
    "adversarial_scenarios",
    "evidence",
    "regressions",
    "recommended_changes",
    "non_changes",
    "merge_disposition"
]

[reporting.finding_schema]
required_fields = [
    "id",
    "severity",
    "location",
    "claim",
    "mechanism",
    "failure_mode",
    "evidence",
    "confidence",
    "recommended_action"
]

[severity]
P0 = "Correctness, safety, or coherence failure. Merge blocker."
P1 = "Architectural or concurrency defect requiring resolution or explicit waiver."
P2 = "Non-blocking defect, maintainability issue, or missing evidence."
P3 = "Observation or future improvement."

[merge_policy]
block_on = ["P0"]
require_explicit_decision_for = ["P1"]
document_only = ["P2", "P3"]

[non_changes]
required = true
description = """
The reviewer must explicitly list things that should NOT be changed, especially
code whose modification would be out of scope for this pass. This includes:

- MemoryCache's RwLock-based implementation (interim L1; Cachelito adoption
  is a separate implementation task, not a review finding).
- Cache trait signatures (changing them to accommodate future primitives is
  out of scope).
- HierarchicalCache's composition structure (unless it has a P0 defect).
- Any existing test or CI configuration.
"""

[performance_claims]
require_evidence = true
accepted_evidence = [
    "source-level concurrency guarantees",
    "existing benchmarks",
    "new benchmark",
    "contention analysis",
    "documented primitive semantics"
]

[final_question]
value = """
Does the current Cache trait + HierarchicalCache composition preserve the
concurrency characteristics required for future adoption of specialized
concurrent primitives (Cachelito for L1) without requiring architectural
changes that would "flatten" the hierarchy?

Additionally: does the current implementation preserve generation-based
coherence and avoid race conditions, contention hazards, and lock amplification
under concurrent reads, mutations, promotion, and invalidation?
"""
```

---

### File 2: `.kilo/plans/PR24_ADVERSARIAL_PASS3_PROMPT.txt`

```text
You are an adversarial systems-engineering reviewer operating on
Metis-Avionics/theDAF PR24.

Your task is Pass 3 of the adversarial review.

This is NOT a normal code review.
This is NOT a refactoring task.
This is NOT a request to improve the architecture according to your own
preferences.
This is NOT a request to adopt Cachelito or DashMap.

Your job is to determine whether PR24 preserves the intended architecture
and whether the current cache abstraction is compatible with future adoption
of specialized concurrent primitives (Cachelito for L1, concurrent map for
potential future tiers) WITHOUT requiring architectural changes that would
"flatten" the hierarchy.

PRIMARY QUESTION

Does the current Cache trait + HierarchicalCache composition preserve the
concurrency characteristics required for a read-heavy, highly concurrent cache
architecture? Specifically: if Cachelito were dropped into L1 tomorrow, would
HierarchicalCache preserve its per-key concurrent read semantics? Would the
mutation/invalidation paths still be correct?

HARD CONSTRAINT

DO NOT rewrite Cachelito. (It does not exist yet; this forbids recommending
its adoption in a way that requires redesigning the hierarchy.)

DO NOT replace Cachelito with HashMap, RwLock<HashMap>, Mutex<HashMap>, or
another generic cache implementation when it IS eventually adopted.

DO NOT replace DashMap with a globally locked HashMap.

DO NOT introduce a global mutex around HierarchicalCache as a convenience fix.

DO NOT recommend rewriting cache primitives merely because another primitive is
easier to reason about.

DO NOT recommend adopting Cachelito or DashMap in this pass. This pass is a
read-only audit of whether the abstraction is ready.

The purpose of this review is to verify that the hierarchy COMPOSES specialized
cache primitives correctly — or identify where it would fail to do so.

CURRENT STATE

L1: MemoryCache — uses Arc<RwLock<MemoryCacheInner>> with HashMap + LRU + trie.
    This is an interim implementation. The architectural plan specifies Cachelito
    as the eventual L1 backend. DO NOT "fix" MemoryCache in this pass.

L2: MokaCache — uses moka::future::Cache (async-native, concurrent).

L3: RedisCache — stub behind feature flag.

L4: PostgresCache — stub behind feature flag.

HierarchicalCache: composes four Arc<dyn Cache> tiers. L1 reads are async;
L2/L3/L4 are fallback on L1 miss. Promotion copies lower-tier hits into L1.

Generation: per-resource, stored in cache, compared in query() to reject stale values.

LockRegistry: 16-shard striped tokio::sync::Mutex for per-resource serialization
of generation advancement.

SYSTEM MODEL

Treat the intended cache architecture as:

    repository
        |
        +--> generation
        |
        +--> L1
        |     |
        |     +--> Cachelito / read-heavy hot path (PLANNED, not current)
        |
        +--> L2
        |
        +--> L3
        |
        +--> L4

The intended read path is:

    L1 -> L2 -> L3 -> L4

The intended mutation model is:

    repository mutation
        -> generation advancement
        -> cache invalidation

The repository and generation are authoritative.

Caches are advisory.

A cache value from an obsolete generation must never be accepted as current.

REVIEW METHOD

1. Inspect PR24's current head.
2. Inspect the complete PR diff.
3. Inspect PR comments and previous adversarial findings.
4. Inspect the living architecture documents.
5. Search the entire repository for:
   - Cachelito (note: not present)
   - DashMap (note: not present)
   - HierarchicalCache
   - Generation
   - delete_prefix
   - shake
   - promotion
   - invalidation
   - cache trait implementations
   - locks
   - RwLock
   - Mutex
   - tokio::sync
   - std::sync
6. Trace the complete read path through the CURRENT implementation.
7. Trace the complete mutation/invalidation path.
8. Trace promotion from lower tiers into L1.
9. Trace generation validation.
10. Inspect concurrent behavior at every boundary.
11. For each boundary, determine: "If L1 were Cachelito (concurrent, per-key),
    would this boundary preserve its concurrency guarantees?"

CONCURRENCY ANALYSIS

For every hot-path operation determine:

- what synchronization is acquired
- its scope
- whether it is per-key, per-tier, or global
- whether independent reads can proceed concurrently
- whether an async operation holds synchronization across await
- whether promotion creates contention
- whether invalidation creates contention
- whether unrelated keys contend
- whether allocations/cloning occur
- whether the abstraction weakens the primitive underneath it

Explicitly test these conceptual workloads:

1. Many readers, one hot key.
2. Many readers, many independent keys.
3. Readers concurrent with mutation.
4. Multiple simultaneous promotions.
5. Concurrent invalidation of different keys.
6. Concurrent invalidation of same key.
7. Prefix invalidation during reads.
8. Full cache shake during reads.
9. Moka degraded prefix invalidation.
10. Concurrent mutation storms.

CACHE PRIMITIVE COMPATIBILITY AUDIT

For Cachelito (PLANNED L1, NOT CURRENT):

Cachelito's documented concurrency model is per-key concurrent reads with
internal sharding. Determine whether the current abstraction would preserve
this model:

- Does HierarchicalCache::get() acquire a lock that serializes independent
  L1 reads?
- Does promotion (L2/L3/L4 -> L1) acquire a lock that would serialize
  concurrent promotions or block L1 readers?
- Does invalidation (delete_prefix, shake) acquire a lock that would
  serialize concurrent reads?
- Does the Cache trait's async signature force await points that would
  serialize concurrent operations in a way that a synchronous Cachelito
  would not?
- Would HierarchicalCache's composition add a synchronization layer that
  defeats Cachelito's native concurrency?

For DashMap (HYPOTHETICAL, NOT CURRENT):

DashMap provides sharded concurrent map access. Determine whether the current
abstraction would preserve DashMap's concurrency model IF it were added:

- Would HierarchicalCache's get() path serialize DashMap's native concurrent
  reads?
- Would promotion or invalidation amplify locking beyond DashMap's shard-level
  granularity?
- Would the Cache trait's async boundary force unnecessary serialization?

DO NOT recommend adopting Cachelito or DashMap. Only report whether the
abstraction is compatible.

FAILURE ANALYSIS

Be especially hostile to this sequence:

    repository mutation succeeds
        ->
    cache invalidation partially fails
        ->
    generation does not advance
        ->
    lower-tier stale value remains
        ->
    subsequent reader accepts stale value

Determine whether the CURRENT implementation can produce this or an equivalent
sequence. (Note: Pass 1/2 already fixed the broken transaction boundary; verify
the fix holds under concurrency.)

Also investigate:

    read
      ->
    lower-tier hit
      ->
    generation validation
      ->
    concurrent mutation
      ->
    promotion
      ->
    L1 insertion

Determine whether a stale value can be resurrected after invalidation when
multiple readers race with a mutation.

DOCUMENTATION AUDIT

Compare implementation against:

- architecture docs
- handover
- BUGS.md
- verification checklist
- index
- ADRs
- PR comments
- plans

Flag contradictions.

Do not treat documentation claims as evidence unless the implementation or
tests support them.

PERFORMANCE DISCIPLINE

Do not make unsupported claims such as:

"DashMap is faster."

"Cachelito is faster."

"RwLock is slower."

Instead establish the mechanism.

If benchmark evidence exists, use it.

If it does not exist, state that the performance property is inferred from
the primitive's documented/source-level concurrency model rather than measured.

REVIEW STANDARD

A green test suite is NOT sufficient.

A passing compiler is NOT sufficient.

Clippy is NOT sufficient.

Power-of-Ten compliance is NOT sufficient.

The question is whether the system remains correct and architecturally coherent
under composition.

DISTINGUISH:

    local correctness
from
    compositional correctness

The most valuable findings will probably occur at subsystem boundaries.

DO NOT FIX CODE UNLESS EXPLICITLY ASKED.

This pass is a forensic/adversarial review.

Produce findings, evidence, failure mechanisms, severity, confidence, and
recommended remediation.

Do not silently modify the repository.

OUTPUT

Return:

1. Executive verdict.
2. Current architectural model (including interim L1 status).
3. Cachelito compatibility assessment (forward-looking; Cachelito is not current).
4. DashMap compatibility assessment (forward-looking; DashMap is not current).
5. HierarchicalCache contention analysis.
6. Read-path trace.
7. Promotion analysis.
8. Invalidation/concurrency analysis.
9. Generation coherence analysis.
10. Degraded-backend analysis.
11. Adversarial scenario results.
12. Documentation contradictions.
13. Findings classified P0/P1/P2/P3.
14. Explicit list of things that should NOT be changed.
15. Evidence supporting every substantive finding.
16. Merge disposition.

For every finding use:

ID:
SEVERITY:
LOCATION:
CLAIM:
MECHANISM:
FAILURE MODE:
EVIDENCE:
CONFIDENCE:
RECOMMENDED ACTION:

Be particularly careful not to confuse:

"the abstraction works"

with:

"the abstraction preserves the concurrency guarantees of the primitive it
abstracts."

That distinction is the central question of this pass.
```

---

## Execution order

1. Create `.kilo/plans/PR24_ADVERSARIAL_PASS3.toml`
2. Create `.kilo/plans/PR24_ADVERSARIAL_PASS3_PROMPT.txt`
3. (Optional, out of scope for this plan) Update `SESSION.md` and `HANDOVER.md` to reference Pass 3 once the review is executed

## Risks

- **Cachelito/DashMap not yet in codebase.** The spec tests whether the current
  `Cache` trait and `HierarchicalCache` composition are ready to receive these
  primitives without architectural changes. Findings may conclude that the
  abstraction is already suitable or that specific boundaries need preservation.
- **Spec-driven agent may over-interpret.** The TOML and prompt explicitly forbid
  primitive rewrites. The executing agent must respect this even if it believes
  a generic primitive is "simpler."
