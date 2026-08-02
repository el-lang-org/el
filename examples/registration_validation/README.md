# Registration validation: EL versus Rust

This example implements the same registration-validation rules in EL and Rust:

- usernames contain 3 to 20 Unicode scalar values (code points);
- an email contains one `@`, non-empty local and domain parts, and a dot in the domain;
- age is between 13 and 120 inclusive; and
- an optional referral code contains exactly 6 Unicode scalar values.

The example is deliberately dependency-free. It highlights EL's structural
unions, exhaustive pattern matching, immutable structs, interpolation, basic
string operations, `with` result propagation, and explicit Unicode string views.
The Rust version is ordinary safe Rust using borrowed strings and its standard
`Result` and `Option` types.

The email rule is intentionally illustrative rather than RFC 5322 validation.

## Run the EL version

From this directory:

```sh
el check --locked
el build --locked
executable="$(find build -type f -path '*/debug/registration_validation' -perm -111 -print -quit)"
"$executable"
```

## Run the Rust version

```sh
rustc rust/main.rs -o /tmp/registration-validation-rust
/tmp/registration-validation-rust
```

Both programs print:

```text
accepted: Mina
rejected: username has 2 code points (expected 3..20)
rejected: invalid email nora.example.com
rejected: age 12 is outside 13..120
rejected: referral has 5 code points (expected 6)
```

When comparing line counts, compare the complete source files and state the
counting rule. In particular, do not omit Rust's type declarations or EL's
explicit output handling.
