# ARM64 runner capacity

The ARM64 workflow uses the organization pool `lampos-arm64-16`: a
GitHub-hosted Ubuntu 22.04 ARM64 image with 16 CPU cores. The operating
system, architecture, Rust version, release profile, test selection and
one-test-at-a-time execution remain the same. The pool has no alternate
runner type.

The owner approved paid capacity on 15 October 2026; the instruction is
recorded on [#788](https://github.com/autonomous-ai/lampOS/issues/788#issuecomment-6188635550).
The organization now has Team. The pool was created at 16:45 +07 on
15 October 2026 and GitHub reported it ready at the 16:46 check; times
were recorded with `date`. The account-side setup is separate from the
workflow change.

Pool ID `1` uses GitHub image `5287` (Ubuntu 22.04, `linux-arm64`) and
machine size `arm-16-core`: 16 CPU cores, 64 GB memory and 600 GB storage.
Its maximum concurrency is 40 jobs, with static public IPs disabled.
Group `lampos-arm64` (ID `3`) allows all public and private repositories
in `autonomous-ai`, as the owner requested. Other teams can select it with:

```yaml
runs-on: lampos-arm64-16
```

The 40-job ceiling is shared across those repositories. Concurrent work
from other teams can affect queue time and must be included in latency
measurements. Do not substitute an x64 image or Ubuntu 24.04: the release
artifacts must retain the existing glibc compatibility with the lamps'
Debian 12 installation.

GitHub's published Linux ARM64 16-core rate is **$0.026 per runner minute**
at the time of the setup check. Idle provisioned capacity has no charge;
executed larger-runner jobs are billed even for public repositories.
Forty busy runners cost at most $1.04 per running minute at that rate;
the concurrency limit is not a monthly spending limit. The organization
plan has its own cost. See
[runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing)
and [runner API](https://docs.github.com/en/rest/actions/hosted-runners).

Before accepting the change, record its actual runner image/architecture,
queue wait, compilation and execution times, and compare the executed test
identities. A larger core count alone does not establish the ten-minute
push-to-result target. No larger-runner measurement is available yet.
