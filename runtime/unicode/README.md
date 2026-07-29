# Bundled Unicode data

EL v1 pins Unicode 17.0.0 and UAX #29 revision 47. The versioned directory
contains the authoritative inputs used by
`tools/generate_unicode_grapheme_tables.py` and the official grapheme-break
conformance corpus. Generated C tables are checked in under `el-runtime` so a
normal build does not depend on Python, the network, the host locale, or an
installed Unicode library.

The files were downloaded from the Unicode Consortium's versioned 17.0.0 UCD
directories. Their SHA-256 values are recorded in the generated table header.
Redistribution is covered by `LICENSE.txt` in this directory.
