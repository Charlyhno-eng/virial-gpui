# Memory measurements

Measured on Linux against `da8dc34`, using optimized builds and three separate
processes per scenario. Values are median increases in resident RAM (RSS),
not total application memory or GPU memory.

| Scenario | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Search catalog, 1,000,000 entries | 123.3 MiB | 62.2 MiB | 49.5% |
| Editor, 60 large document replacements | 201.1 MiB | 11.3 MiB | 94.4% |
| Syntax colors, 40,000 source lines | 85.8 MiB | 12.5 MiB | 85.4% |
| Animation, 60 solid-color 960 × 540 frames | 119.1 MiB | 1.5 MiB | 98.7% |

The syntax-color measurement used the production module without test-only
document metadata. Animation frames in this fixture compress especially well;
photographs and noisy frames will save less. These independent figures should
not be added together to estimate normal browsing memory.

Search names share packed allocations, with lowercase names reused as matching
keys. Syntax spans retain RGB colors rather than full style objects. Local edits
preserve untouched line allocations; large undo payloads use lossless compression
while keeping all 500 steps. Raster assets also use lossless compression when it
saves at least 12.5%; pixels are decoded for texture upload, rather than every
animation tick. Single-frame video updates retain their existing raw-pixel path.
SSH, search scope, cache compatibility, image resolution and timing are preserved.
Compression trades some asset-loading CPU time for lower retained RAM.

Reproduce the isolated benchmarks separately:

```sh
cargo test --release --locked indexed_search_performance -- --ignored --nocapture
cargo test --release --locked editor_memory_performance -- --ignored --nocapture
cargo test --release --locked animation_memory_performance -- --ignored --nocapture
```

`code_memory_performance` also provides a regression benchmark, but its numbers
include test-only metadata and differ from the production figures above.

Validation: 486 tests passed, 19 manual tests were ignored, and Clippy and formatting
checks passed. The isolated D-Bus monitor test was excluded because this sandbox
forbids socket creation; the unchanged baseline fails for the same reason.

**Desktop verification remains pending.** Both GUI launch attempts failed before
opening a window: X11 socket access returns `Operation not permitted` in this
sandbox. A desktop run must still compare idle browsing, search, raster previews
and editing, including GPU/driver memory and the return to idle after closing
previews. No claim about total GUI RAM consumption is made here.

## Idle browsing follow-up

The reported desktop baseline is approximately 116 MB at rest. The isolated
figures above do not demonstrate a reduction of that baseline.

Row callbacks now share a path and fetch an entry when invoked, rather than
retaining cloned names and full entries. Directory timestamps remain unformatted
until a row is displayed. Filtered listings release unused entry capacity and
metadata. Refresh selection restoration borrows paths from the previous listing.

On Linux with glibc, a worker calls `malloc_trim(0)` after two seconds without
render activity, once directory loading, size scans and operations have settled.
It returns freed allocator pages without discarding live objects. Completed size
tasks are released so this idle check can finish. This does not run on Windows,
macOS or non-glibc targets, and does not change SSH code or platform backends.

An isolated release benchmark retains 25 of 400 temporary 60 KiB blocks and
checks that their bytes survive reclamation. Across three processes, median RSS
after freeing the temporary blocks was 29,760 KiB; after reclamation it was
7,680 KiB (21.6 MiB returned), with median reclamation time 1.4 ms including
worker creation. This fixture validates reclamation of free pages, **not** the
complete GUI's idle RAM or its visual behavior.

```sh
cargo test --release --locked idle_allocator_memory_performance -- --ignored --nocapture
```

The previous release binary was saved as
`/tmp/virial-ram-analysis/virial-before-idle`. Compare it with
`target/release/virial-gpui` on the same desktop, directory, window size and
preview state, allowing background size scans to finish and sampling `/proc/PID/smaps_rollup`
for RSS and PSS. Then verify Right/Left navigation into loaded, loading and empty
folder columns, including active headers and restored selections. X11/GPU access
is still required to complete this comparison.
