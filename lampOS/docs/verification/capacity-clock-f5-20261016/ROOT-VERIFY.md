# Read-only root verification

From the evidence directory:

```sh
cd /private/tmp/tests-823-capacity-f5-publication
shasum -a256 -c SHA256SUMS
```

Verify the two current source files plus all 14 historical linked files:

```sh
cd /private/tmp/lampos-tests-capacity-f5
shasum -a256 -c /private/tmp/tests-823-capacity-f5-publication/SOURCE-HISTORY-SHA256SUMS
git diff --check
git status --short --untracked-files=all
```

Read `f5-gate/terminal-audit/independent-result.json`, candidate and baseline
`gates.json`, `candidate/compiled-identity-proof.json`, the two selected logs,
the full log's terminal summary, build/docs exits, and source inverse proof.
Use the external publication manifest for exact path/hash/mode staging if
publication is later authorized. It also lists ignored `.log` paths which
need exact forced adds; never stage broadly. There is no need to run Cargo,
fetch a new baseline, clean a target or repeat any test to verify this package.
