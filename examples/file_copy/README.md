# File copy example

This EL v1 program reads `input.txt` in 16-byte chunks and writes those bytes
to `output.txt`. It demonstrates recoverable file operations, the `Reader` and
`Writer` protocols, end-of-file handling, flushing, and deterministic cleanup
with `defer`.

From this directory, build and run it with:

```sh
el build
executable="$(find build -type f -path '*/debug/file_copy' -perm -111 -print -quit)"
"$executable"
cat output.txt
```

The program exits with status 0 after a successful copy and status 1 after an
I/O error. Errors are written to standard error.
