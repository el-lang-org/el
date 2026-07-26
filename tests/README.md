# Test and golden-file conventions

Tests live at the narrowest crate boundary that owns the behavior. Accepted
and rejected cases stay adjacent, source expectations use package-relative
paths, and output must not depend on host paths, allocation addresses, hash
order, locale, or time.

Deterministic human-readable output is stored as checked-in files under a
crate's `tests/golden/` directory. Tests compare bytes with the small helper in
that crate's `tests/support/` module. Golden files are updated deliberately in
reviewed patches; tests never rewrite them automatically.

End-to-end fixtures will live under `tests/e2e/`, grammar/type conformance under
`tests/conformance/`, and reusable EL projects under `tests/fixtures/` when
their owning milestones begin.

