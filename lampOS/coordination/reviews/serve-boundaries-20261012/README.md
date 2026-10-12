# Independent serve-runner boundary review

Review base: `8c09252649b5923ded19cffc7a3fc9bd0a01927d`.
These patches are audit reproductions, not release changes. Apply them to a
private copy of `lampOS` at that base; the first adds a test-only clock seam to
the otherwise unchanged grant path. No hardware, audio, cloud or credentials
are used. They reproduce a grant subtraction panic after descheduling, a serve
stdout readiness shortcut, and clean classification of a nonzero runtime exit.
The second patch adds a real inert child that acknowledges metadata then exits
7; the old relay returns success. Preserve the positive controls. Agent 2's typed
policy-options change may require adding its new config field to test literals.

The original review passed 33 focused positive controls. These intentionally red
regressions must not be treated as a green release checkpoint. Complete results
stay in the root's local review directory; no room recordings are included.
