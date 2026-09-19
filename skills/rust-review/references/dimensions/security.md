# Security

Report vulnerabilities reachable from input an attacker can influence. Name the source and the sink.

## Look for

- **Path traversal.** `base.join(user_input)` accepts `..` and absolute paths (joining an absolute path *replaces* the base). Canonicalise and check the prefix, or reject path components.
- **Command construction.** `Command::new("sh").arg("-c").arg(format!(...))` with input is injection. `Command::new(bin).args([...])` with an argument vector is safe from shell injection. Still check it for option injection (input starting with `-`; use `--`).
- **SQL built with `format!`** instead of bound parameters.
- **Unbounded deserialisation or reads.** Reading or deserialising untrusted data without size limits. Examples: `read_to_end` on a socket, `Vec::with_capacity(len_from_input)`, recursive formats without depth limits.
- **Secrets in logs or errors.** A `Debug` derive on a struct holding tokens or passwords. Logging request headers. Error messages that echo credentials.
- **Disabled TLS verification:** `danger_accept_invalid_certs(true)`, custom verifiers that accept everything.
- **Weak randomness for security:** `rand::thread_rng()` is a CSPRNG in current `rand`, but `SmallRng`, `fastrand`, or time-seeded generators are not. Tokens and nonces need `OsRng` or an equivalent.
- **Timing-unsafe comparison of secrets** (`==` on MAC tags). Use constant-time comparison.

## Do not flag

- Tests and examples.
- Inputs that come only from trusted configuration, once you have confirmed that.

## Evidence that makes it a finding

A source-to-sink path: "the `name` query parameter goes to `serve_file`, then `root.join(name)`; a request for `../../etc/passwd` escapes `root`."
