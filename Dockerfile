FROM rust:1.93.1-bookworm AS build

WORKDIR /workspace
COPY . .

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

RUN cargo build --release -p lsi-daemon -p lsi-cli -p lsi-tui

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system lsi \
    && useradd --system --gid lsi --home-dir /var/lib/night-bridge --create-home --shell /usr/sbin/nologin lsi \
    && install -d -o lsi -g lsi -m 0750 /var/lib/night-bridge

COPY --from=build /workspace/target/release/night-bridge-daemon /usr/local/bin/night-bridge-daemon
COPY --from=build /workspace/target/release/night-bridge /usr/local/bin/night-bridge
COPY --from=build /workspace/target/release/night-bridge-tui /usr/local/bin/night-bridge-tui

USER lsi
WORKDIR /var/lib/night-bridge

ENV XDG_CONFIG_HOME=/var/lib/night-bridge/config
ENV XDG_DATA_HOME=/var/lib/night-bridge/data

VOLUME ["/var/lib/night-bridge"]

EXPOSE 53317 53400 53500 53501

ENTRYPOINT ["night-bridge-daemon"]
