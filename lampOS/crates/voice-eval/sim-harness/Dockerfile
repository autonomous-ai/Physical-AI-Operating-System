FROM rust:1.96.0-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends libclang-dev clang libasound2-dev pkg-config && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy rustfmt
# Containers have no systemd; report every unit as absent (exit 4) for the
# coordinator's legacy-owner guard, exactly as a host without those units.
RUN printf '#!/bin/sh\nexit 4\n' > /usr/bin/systemctl && chmod 755 /usr/bin/systemctl
WORKDIR /src
