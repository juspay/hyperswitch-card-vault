FROM rust:slim-trixie AS builder

WORKDIR /locker

ENV CARGO_NET_RETRY=10
ENV RUSTUP_MAX_RETRIES=10
ENV CARGO_INCREMENTAL=0

# Build argument to determine which features to use
ARG DEV=false

RUN apt-get update \
    && apt-get install -y libpq-dev libssl-dev pkg-config

COPY . .
# Use a conditional to set the features flag based on DEV value
RUN if [ "$DEV" = "true" ]; then \
        cargo build --release --features dev ${EXTRA_FEATURES}; \
        echo "Building with dev features"; \
    else \
        cargo build --release --features release ${EXTRA_FEATURES}; \
        echo "Building with release features"; \
    fi


FROM debian:trixie-slim

ARG CONFIG_DIR=/local/config
ARG BIN_DIR=/local
ARG BINARY=locker

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata libpq-dev curl procps

EXPOSE 8080

RUN mkdir -p ${CONFIG_DIR}

COPY --from=builder /locker/target/release/${BINARY} ${BIN_DIR}/${BINARY}

WORKDIR ${BIN_DIR}

CMD ["./locker"]

