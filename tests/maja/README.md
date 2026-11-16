# Maja Wave 1 admission qualification

These independent black-box tests encode the Wave 0 fail-closed policy in
`docs/maja/contracts/maja-web-1.md`: no build profile is currently admitted.
The case inventory is a golden disposition list, not generated output or
evidence that any source feature works in a browser.

Run `MAJA_BIN=/absolute/path/to/maja python3 -m unittest discover -s tests/maja -v`.
Without `MAJA_BIN`, compiler-dependent tests are explicitly skipped; a skipped
suite is not a passing compiler qualification. The command assumes the CLI
surface specified in the directive and checks failure, diagnostics,
determinism and absence of published files. The golden matrix must be revised
when a profile is explicitly admitted. Source-origin precision, structured
diagnostic schema, security, accessibility, D1 differential behaviour and
actual browser execution require separate tests and remain unqualified.
