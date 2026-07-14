# Hazard: concurrent Claude sessions + git mutations on shared working tree

This repo (openticker) is sometimes worked on by MULTIPLE concurrent Claude Code sessions
sharing the SAME git working tree on `main`. One session commits frontend/config-editor work
and runs `git reset` operations.

## What went wrong (2026-06-11)
During the audit-fix work, a SUBAGENT ran `git stash` (to compare a baseline) and the pop
conflicted. Combined with the concurrent session's commits/resets, this wiped ~10 crates of
uncommitted audit work from the working tree. HEAD had also advanced from 217a5e2 to 31c9c00.

## Recovery (worked)
- `git fsck --no-reflogs --unreachable | grep commit` surfaced dangling stash commits
  ("WIP on main: ...", "index on main: ...").
- Identified the newest WIP stash by date, inspected `git diff --stat <stash>^1 <stash>`,
  found it contained the full superset of work.
- Preserved it as a branch: `git branch recovery-stash-<sha> <stash>`.
- Restored files with `git checkout <stash> -- <paths>` (excluding any file the current
  on-disk version was newer for). Submodule (openticker-indicators) working-tree changes
  were unaffected by the parent reset.

## Rules to prevent recurrence
1. NEVER let subagents run git MUTATING commands (stash/reset/checkout/clean/commit/rebase).
   Always tell dispatched subagents: "Do NOT run any git command that mutates state; only
   read (git status/diff/log) and edit files." Verification (build/test/clippy) is fine.
2. The concurrent session's commits only touched `ui/*.vue` files — benign to Rust audit work.
3. If the working tree looks wiped, check `git fsck --unreachable` for stash commits before
   assuming loss; uncommitted changes that were ever stashed survive as dangling objects.
