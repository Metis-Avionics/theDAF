# AGENTS_SYNC.md — TCAS (Traffic Collision Avoidance System)

## Purpose
Multiple agents may work on the same repo concurrently. This file prevents
file-level and semantic collisions without requiring a central scheduler.

## Agent Identity
Each agent MUST identify itself at the top of every message using the pattern:

    Agent-<work-tag>-<random-suffix>

- `<work-tag>` is derived from the current task (e.g. `pr18-redteam-r2`, `barrel-opt`, `dp-refactor`).
- `<random-suffix>` is any unique 4-char token (e.g. `a3f9`).
- Example: `Agent-pr18-redteam-r2-a3f9`

## File Ownership Rules

1. **Claim before edit.** Before editing a file, announce the file path and the
   change intent in your message. Other agents must not edit that file for the
   same logical change without coordination.

2. **Read-only for others.** If Agent X announces it is editing `src/foo.py`,
   other agents must treat `src/foo.py` as read-only until Agent X reports
   completion or releases the claim.

3. **Release claim.** After finishing edits to a file, explicitly state that
   the claim is released. Do not hold claims across idle turns.

4. **New files.** Announce creation of new files in the same way. Other agents
   must not modify new files created by a peer without coordination.

5. **Test files.** Test files that exercise a component are considered part of
   that component's ownership. Coordinate before modifying tests written by
   another agent.

## Conflict Resolution

- **Same file, different intent:** Agents must coordinate via direct message
  before proceeding. Do not silently overwrite.
- **Same file, same intent:** The first agent to announce the edit proceeds;
  others wait or split the file by section.
- **Merge conflict:** Stop, surface the conflict, and ask for human direction.

## Status Broadcast

At the end of every turn, broadcast:

- Files currently claimed (if any)
- Files just released
- Next intended file(s)

## Identification Banner

Start every response with:

    [Agent-<work-tag>-<suffix>] Status: <active | idle | blocked>
