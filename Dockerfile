# Stage 1: Build
FROM rust:1.77-alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    gcc \
    build-base \
    pkgconfig \
    openssl-dev

WORKDIR /build

# Cache dependency compilation
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src

# Build the actual binary
COPY . .
RUN cargo build --release
RUN strip target/release/dreamswarm

# Stage 2: Runtime
FROM alpine:3.19

RUN apk add --no-cache \
    tmux \
    git \
    ripgrep \
    bash \
    ca-certificates \
    tini

# Create non-root user
RUN addgroup -S dreamswarm && adduser -S dreamswarm -G dreamswarm
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
