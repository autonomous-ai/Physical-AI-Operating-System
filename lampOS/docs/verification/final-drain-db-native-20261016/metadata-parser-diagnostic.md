# Preserved metadata-parser correction

All initial network captures returned 0. The first normal audit had already
verified the result rows and separate doc/own-harness cases when its source
check stopped: it incorrectly required the `sha` field from a recursive GitHub
tree request made with a commit SHA to equal that commit's canonical tree SHA.
Those commit-ref responses echoed the requested commit SHA.

The Git commit objects supplied the canonical trees:

- db head: `fb26a6fe50a70f7ef2906dab422e8ec52466afaf`.
- normal merge: `8ee3a47b132b9942dfc502f0ffaa67b0566b16f7`.

The subsequent captures used those canonical tree endpoints. The corrected
audit verified that the recursive entries are byte-for-byte equivalent as JSON
values to the original commit-ref tree entries and that the canonical head tree
matches local Git. Both raw response forms, capture receipts, the first parser
stderr/exit and the separate successful parser result remain intact. No source,
test, original log, job or workflow run was changed or rerun.
