# License policy

The EL repository does not yet declare a project license. A project license
must be selected before distributing compiler binaries or source releases.

Third-party source may be added only when its exact version, source revision,
archive checksum, upstream URL, and license notice are checked in. The CI
license job verifies this metadata for every vendored component. Rust
dependencies must be exact in `Cargo.lock`; dependency license review will be
extended when the first external Rust dependency is introduced.

Milestone 0 vendors only the Boehm-Demers-Weiser collector archive. Its notice
is in `runtime/vendor/boehm-gc/LICENSE` and its pin is in
`runtime/vendor/boehm-gc/README.md`.

