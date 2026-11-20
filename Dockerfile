FROM rust:1-trixie

# --- System packages -------------------------------------------------------
# qemu-system-arm is kept as an optional second opinion for core-only testing
# (`make qemu`); Renode is the primary simulator.
RUN apt-get update && apt-get install -y --no-install-recommends \
        qemu-system-arm \
        gdb-multiarch \
        binutils-arm-none-eabi \
        make \
        git \
        wget \
        ca-certificates \
        python3 \
        python3-pip \
        python3-venv \
        sudo \
    && rm -rf /var/lib/apt/lists/*

# --- Renode (portable Linux build, bundles the dotnet runtime) --------------
# Bump RENODE_VERSION to upgrade. Release assets:
#   https://github.com/renode/renode/releases
ARG RENODE_VERSION=1.17.0
RUN mkdir -p /opt/renode \
    && wget -q "https://github.com/renode/renode/releases/download/v${RENODE_VERSION}/renode-${RENODE_VERSION}.linux-portable.tar.gz" -O /tmp/renode.tar.gz \
    && tar xf /tmp/renode.tar.gz -C /opt/renode --strip-components=1 \
    && rm /tmp/renode.tar.gz \
    && ln -s /opt/renode/renode /usr/local/bin/renode \
    && ln -s /opt/renode/renode-test /usr/local/bin/renode-test

# --- nRF52840 SVD, baked into the image -------------------------------------
# Renode's stock nRF52840 platform fetches this SVD over HTTP on every fresh
# container. Download it once at build time and point the platform at the local
# copy. The build fails loudly if the URL in the stock platform ever changes.
ARG SVD_URL=https://dl.antmicro.com/projects/renode/svd/NRF52840.svd.gz
RUN mkdir -p /opt/renode/svd \
    && wget -q "${SVD_URL}" -O /opt/renode/svd/NRF52840.svd.gz \
    && gunzip -f /opt/renode/svd/NRF52840.svd.gz \
    && sed -i 's|@https://dl.antmicro.com/projects/renode/svd/NRF52840.svd.gz|@/opt/renode/svd/NRF52840.svd|' \
         /opt/renode/platforms/cpus/nrf52840.repl \
    && grep -q '@/opt/renode/svd/NRF52840.svd' /opt/renode/platforms/cpus/nrf52840.repl

# Robot Framework + deps for `renode-test`, in a venv (Debian blocks global pip).
RUN python3 -m venv /opt/venv \
    && ( [ -f /opt/renode/tests/requirements.txt ] \
         && /opt/venv/bin/pip install --no-cache-dir -r /opt/renode/tests/requirements.txt \
         || /opt/venv/bin/pip install --no-cache-dir robotframework )

# Avoid needing ICU libraries for the bundled dotnet runtime.
ENV DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1
ENV PATH="/opt/venv/bin:/usr/local/cargo/bin:${PATH}"

# --- Rust toolchain ---------------------------------------------------------
# Cortex-M4F target + components. Keep in sync with rust-toolchain.toml.
RUN rustup default stable \
    && rustup target add thumbv7em-none-eabihf \
    && rustup component add rust-src llvm-tools clippy rustfmt rust-analyzer \
    && cargo install --locked cargo-binutils

# World-writable cargo home so the container can run as the host user
# (avoids root-owned target/ directories on the host).
ENV CARGO_HOME=/cargo-home
RUN mkdir -p /cargo-home && chmod 777 /cargo-home

# --- GitHub CLI --------------------------------------------------------------
# From GitHub's own apt repo: Debian's `gh` package lags well behind. The repo is
# left configured, so `sudo apt-get update && sudo apt-get install --only-upgrade gh`
# upgrades it later.
RUN mkdir -p -m 755 /etc/apt/keyrings \
    && wget -q https://cli.github.com/packages/githubcli-archive-keyring.gpg \
         -O /etc/apt/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
         > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends gh \
    && gh extension install github/gh-stack \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for the dev container (.devcontainer/devcontainer.json). VS Code
# remaps its UID/GID to match yours, so files it creates are not root-owned.
# The Makefile ignores this user and keeps using `docker run --user <your uid>`.
RUN useradd --create-home --uid 1000 --shell /bin/bash dev

# Passwordless sudo so you can `sudo apt-get install ...` inside the dev
# container. This is a local, single-user dev image; do not publish it.
RUN echo 'dev ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/dev \
    && chmod 0440 /etc/sudoers.d/dev

# Mount point for Claude Code state (conversations, settings, login). It is
# created here, owned by `dev`, so the named volume that
# .devcontainer/devcontainer.json mounts on it is initialised writable by `dev`
# rather than root-owned.
RUN mkdir -p /home/dev/.claude && chown dev:dev /home/dev/.claude

# Same for the GitHub CLI's login (`gh auth login`). ~/.config is chowned too,
# since mkdir -p would otherwise leave it root-owned.
RUN mkdir -p /home/dev/.config/gh && chown -R dev:dev /home/dev/.config

# --- Extra packages that survive image rebuilds -----------------------------
# `sudo apt-get install` works at runtime but is lost when the container is
# rebuilt. To keep a package, add it to .devcontainer/extra-packages.txt and
# rebuild. This layer is last so editing the list does not re-run the slow steps
# above.
COPY .devcontainer/extra-packages.txt /tmp/extra-packages.txt
RUN pkgs="$(grep -v '^[[:space:]]*#' /tmp/extra-packages.txt | xargs)" \
    && if [ -n "$pkgs" ]; then \
         apt-get update \
         && apt-get install -y --no-install-recommends $pkgs \
         && rm -rf /var/lib/apt/lists/*; \
       fi \
    && rm /tmp/extra-packages.txt

WORKDIR /work
CMD ["bash"]
