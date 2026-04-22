# openticker-lane

Last reviewed: 2026-04-21

Per-lane runtime state and lane-local DTOs extracted from
`openticker-runtime`, including lane state storage and indicator/strategy
construction adapters, lane identity and bootstrap-state recovery helpers,
bootstrap lane fanout/build helpers, inventory and fill-state helpers, recovery validation, the pure
signal-evaluation and strategy-preparation kernels, and the shared lane-cycle
and lane-polling workflow entrypoints driven through runtime-provided
side-effect ports, plus the warmup backfill and pending-warmup workflow
entrypoints, the execution/journaling sub-workflow entrypoints, and the
manual-close workflow used by runtime manual operations.
