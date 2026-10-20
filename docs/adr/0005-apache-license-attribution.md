# 5. Apache-2.0, behavior derived from Clipman, attribution

## Status

Accepted.

## Context

The original Clipman is Apache-2.0: a permissive license whose only real
obligation is keeping the license text and notices. Its sensitive-detection
module and its test corpus (568 benign and 167 secret samples) are the result of
calibration work we want to reuse rather than repeat, and the behavior of the
whole application is the specification this port follows.

The COSMIC applet template carries no license file; it only substitutes an SPDX
identifier, defaulting to MPL-2.0, and libcosmic is MPL-2.0 as well. MPL-2.0 in
the same binary as Apache-2.0 code is possible but adds an obligation layer
(file-level copyleft) that buys nothing here.

## Decision

License this repository Apache-2.0, the same as the original. Keep a `NOTICE`
file naming Clipman, its copyright holder and the files ported from it.
Translate behavior and rules — the sensitive patterns and their corpus — with
the copyright headers preserved, and state the derivation in the README.

## Consequences

- Ported rules and fixtures are legally boring: identical license, no boundary
  to document.
- libcosmic stays an MPL-2.0 dependency, unmodified, which its license allows
  from an Apache-2.0 project as long as libcosmic's own files stay MPL-2.0.
- The README must not imply endorsement by the original author or by System76;
  this is an independent port.
