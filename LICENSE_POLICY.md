# License policy

The EL repository does not yet declare a project license. This is an explicit
EL v1 release blocker: compiler binaries and source releases must not be
published until the project owners select and add one. Milestone 9 technical
conformance does not imply permission to distribute.

Third-party source may be added only when its exact version, source revision,
archive checksum, upstream URL, and license notice are checked in. The CI
license job verifies this metadata for every vendored component. Rust
dependencies must be exact in `Cargo.lock`; dependency license review will be
extended when the first external Rust dependency is introduced.

Milestone 0 vendors only the Boehm-Demers-Weiser collector archive. Its notice
is in `runtime/vendor/boehm-gc/LICENSE` and its pin is in
`runtime/vendor/boehm-gc/README.md`.
