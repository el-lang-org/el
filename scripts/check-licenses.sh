#!/bin/sh
set -eu

archive="runtime/vendor/boehm-gc/gc-8.2.12.tar.gz"
notice="runtime/vendor/boehm-gc/LICENSE"
metadata="runtime/vendor/boehm-gc/README.md"
project_notice="LICENSE"
expected="42e5194ad06ab6ffb806c83eb99c03462b495d979cda782f3c72c08af833cd4e"

test -f "$archive"
test -s "$notice"
test -s "$metadata"
test -s "$project_notice"

if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$archive" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$archive" | awk '{print $1}')
fi

if [ "$actual" != "$expected" ]; then
  echo "Boehm GC archive checksum mismatch" >&2
  exit 1
fi

grep -F "$expected" "$metadata" >/dev/null
grep -F "Permission is hereby granted" "$notice" >/dev/null
grep -F "MIT License" "$project_notice" >/dev/null
grep -F "Copyright (c) 2026 EL contributors" "$project_notice" >/dev/null
grep -F "Permission is hereby granted" "$project_notice" >/dev/null
