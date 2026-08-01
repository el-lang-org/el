# Test and golden-file conventions

Tests live at the narrowest crate boundary that owns the behavior. Accepted
and rejected cases stay adjacent, source expectations use package-relative
paths, and output must not depend on host paths, allocation addresses, hash
order, locale, or time.

Deterministic human-readable output is stored as checked-in files under a
crate's `tests/golden/` directory. Tests compare bytes with the small helper in
that crate's `tests/support/` module. Golden files are updated deliberately in
reviewed patches; tests never rewrite them automatically.

Traceable v1 reference fixtures live under `tests/conformance/v1/`. Their first
line records whether the program must be accepted or rejected, its expected
phase and stable diagnostic code when rejected, and the controlling
specification section. Crate-local conformance tests continue to own the dense
boundary matrices; the repository-level fixtures protect representative
end-to-end behavior from `../docs/EXAMPLES.md`.
