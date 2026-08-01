# Unicode report example

This multi-module EL v1 program accepts an optional text argument and reports
its UTF-8 byte count, Unicode scalar count, extended grapheme-cluster count, and
grapheme frequency map. It demonstrates manifests, modules, a derived struct,
tagged results, exhaustive pattern matching, pipelines, generic `Enum`
reduction and callbacks, immutable maps, recursive list traversal, recursive
numeric formatting, buffers, process arguments, Unicode 17 semantics, and
console output.

Its source tree also demonstrates path-derived nested modules: analysis lives
under `src/analysis/`, presentation and formatting live under `src/output/`,
and `src/main.ell` remains the executable entry module.

From this directory, build and run it with:

```sh
el build --locked
executable="$(find build -type f -path '*/debug/unicode_report' -perm -111 -print -quit)"
"$executable"
"$executable" "Café 🇸🇬 🇸🇬 🙂"
```

Use `el build --release --locked` for the optimized executable under the
corresponding `release` directory. The `find` step keeps the command independent
of the host-specific LLVM target-triple directory.
