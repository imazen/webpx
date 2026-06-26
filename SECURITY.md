# Security Policy

`webpx` wraps Google's [libwebp](https://chromium.googlesource.com/webm/libwebp)
C library through FFI and is designed to decode WebP images from untrusted
sources (HTTP bodies, uploaded files). Soundness in the FFI layer is a
first-class concern, and we take vulnerability reports seriously.

## Supported Versions

| Version   | Supported                          |
| --------- | ---------------------------------- |
| 0.4.x     | :white_check_mark:                 |
| <= 0.3.4  | :x: (yanked from crates.io)        |

Every release through `0.3.4` has been yanked from crates.io and contains at
least one known soundness or resource-exhaustion issue. **`0.4.0` is the only
supported version** — upgrade if you are on anything older. Fixes are documented
in [`CHANGELOG.md`](CHANGELOG.md), and reproducer tests for most issues live in
[`tests/soundness.rs`](tests/soundness.rs).

## Reporting a Vulnerability

Please report security issues **privately**, by either:

- emailing **lilith@imazen.io**, or
- using the [Report a Vulnerability button on GitHub](https://github.com/imazen/webpx/security/advisories/new)
  (Security → Advisories → Report a vulnerability).

Refrain from posting anything publicly — issues, PRs, discussions — until we
have verified the report and made a fixed release available. We will acknowledge
your report as quickly as we can, keep you updated as we work toward a fix, and
credit you in the advisory and release notes unless you ask us not to.

A helpful report includes the affected version, a description of the issue, and
— ideally — a minimal reproducer (a WebP file or a short Rust snippet). If you
are unsure whether something qualifies, send it anyway; we would rather triage a
false alarm than miss a real bug.

## Scope

In scope — webpx's own FFI layer over libwebp:

- Memory-safety / soundness bugs reachable from safe Rust: use-after-free,
  out-of-bounds read/write, uninitialized reads, aliasing violations, or a
  panic unwinding through libwebp's C frames.
- Resource exhaustion (DoS) reachable from untrusted input, including any way to
  bypass or defeat the [`Limits`](https://docs.rs/webpx/latest/webpx/struct.Limits.html)
  policy that the decode and metadata paths enforce by default.
- Incorrect or corrupted output — wrong pixels, precision loss, or
  decode/encode divergence — produced from valid input.

Out of scope here — vulnerabilities in **libwebp itself** (the upstream Google
C library) rather than in webpx's bindings. If you find one, please report it to
the [WebP project](https://issues.webmproject.org/) upstream; we are happy to
help route a report if you are unsure which layer is at fault.

## Pure-Rust alternative

If you want to avoid C FFI entirely, [`zenwebp`](https://github.com/imazen/zenwebp)
is a `#![forbid(unsafe_code)]` pure-Rust WebP codec with a matching `zencodec`
trait surface, so the same caller code works against either crate. It is the
recommended choice for new projects and for `wasm32-unknown-unknown` targets.

## Published advisories

Confirmed and fixed issues are published at
<https://github.com/imazen/webpx/security/advisories>.
