FROM rust:1.78.0-bookworm AS build

WORKDIR /workspace
COPY . .

RUN cargo build --release -p lsi-daemon -p lsi-cli -p lsi-tui

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system lsi \
    && useradd --system --gid lsi --home-dir /var/lib/localsend-improved --create-home --shell /usr/sbin/nologin lsi \
    && install -d -o lsi -g lsi -m 0750 /var/lib/localsend-improved

COPY --from=build /workspace/target/release/localsend-improved-daemon /usr/local/bin/localsend-improved-daemon
COPY --from=build /workspace/target/release/localsend-improved /usr/local/bin/localsend-improved
COPY --from=build /workspace/target/release/lsi-tui /usr/local/bin/lsi-tui

USER lsi
WORKDIR /var/lib/localsend-improved

ENV XDG_CONFIG_HOME=/var/lib/localsend-improved/config
ENV XDG_DATA_HOME=/var/lib/localsend-improved/data

VOLUME ["/var/lib/localsend-improved"]

EXPOSE 53317 53400 53500 53501

ENTRYPOINT ["localsend-improved-daemon"]
