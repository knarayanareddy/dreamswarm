# Stage 1: Build
FROM ubuntu:24.04 AS builder

# Prevent tzdata prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install build dependencies for C-bindings and Rust toolchain
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    cmake \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# Build the binary natively! No cross-compiling!
COPY . .
RUN cargo build --release
# The binary is at target/release/dreamswarm

# Stage 2: Runtime
FROM ubuntu:24.04

# Prevent tzdata prompts
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    tmux \
    git \
    ripgrep \
    bash \
    ca-certificates \
    tini \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -ms /bin/bash dreamswarm
USER dreamswarm

WORKDIR /home/dreamswarm

# Copy binary
COPY --from=builder /build/target/release/dreamswarm /usr/local/bin/dreamswarm

# Create data directories
RUN mkdir -p .dreamswarm/memory/topics \
    .dreamswarm/memory/transcripts \
    .dreamswarm/daemon/logs \
    .dreamswarm/teams

ENV DREAMSWARM_DATA_DIR=/home/dreamswarm/.dreamswarm
ENV TERM=xterm-256color

ENTRYPOINT ["tini", "--", "dreamswarm"]
CMD ["--help"]


