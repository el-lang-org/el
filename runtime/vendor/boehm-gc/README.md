# Boehm GC vendoring record

- Upstream: <https://github.com/bdwgc/bdwgc>
- Release: 8.2.12
- Tag: `v8.2.12`
- Annotated tag object: `b5cca9273d23c230fccf2323bdf94b2f0ed21cf5`
- Source revision: `4fab5386df64466b2b61fc7209bef033cad1e6cc`
- Distribution archive: `gc-8.2.12.tar.gz`
- Archive SHA-256:
  `42e5194ad06ab6ffb806c83eb99c03462b495d979cda782f3c72c08af833cd4e`
- Release URL:
  <https://github.com/bdwgc/bdwgc/releases/tag/v8.2.12>
- Archive URL:
  <https://github.com/bdwgc/bdwgc/releases/download/v8.2.12/gc-8.2.12.tar.gz>
- License: permissive Boehm GC notice; see `LICENSE`, whose text is copied
  verbatim from the licensing section at the start of the release's
  `README.QUICK`.

The archive is retained rather than an extracted working tree so that its
upstream checksum remains directly verifiable. Runtime build support will
extract it into an ignored directory. The collector is not compiled or linked
by Milestone 0, and no collector API is exposed to EL source programs.
