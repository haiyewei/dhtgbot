# syntax=docker/dockerfile:1.7

FROM alpine:3.21 AS deps

ARG TARGETARCH=amd64
ARG DHTGBOT_REPO_OWNER=haiyewei
ARG DHTGBOT_REPO_NAME=dhtgbot
ARG DHTGBOT_VERSION=v0.2.1
ARG AMAGI_REPO_OWNER=bandange
ARG AMAGI_REPO_NAME=amagi-rs
ARG AMAGI_VERSION=v0.1.0
ARG TDLR_REPO_OWNER=haiyewei
ARG TDLR_REPO_NAME=tdlr
ARG TDLR_VERSION=v0.2.2

RUN apk add --no-cache ca-certificates curl tar

RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) arch="x86_64" ;; \
      arm64) arch="aarch64" ;; \
      *) echo "unsupported TARGETARCH: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    fetch_release_asset() { \
      url="$1"; \
      output="$2"; \
      curl -fsSL --retry 10 --retry-delay 3 --retry-all-errors "$url" -o "$output"; \
    }; \
    mkdir -p /out; \
    fetch_release_asset "https://github.com/${DHTGBOT_REPO_OWNER}/${DHTGBOT_REPO_NAME}/releases/download/${DHTGBOT_VERSION}/dhtgbot-${arch}-unknown-linux-musl.tar.gz" /tmp/dhtgbot.tar.gz; \
    tar -xzf /tmp/dhtgbot.tar.gz -C /tmp; \
    install -Dm755 /tmp/dhtgbot /out/dhtgbot; \
    fetch_release_asset "https://github.com/${AMAGI_REPO_OWNER}/${AMAGI_REPO_NAME}/releases/download/${AMAGI_VERSION}/amagi-${arch}-unknown-linux-musl.tar.gz" /tmp/amagi.tar.gz; \
    tar -xzf /tmp/amagi.tar.gz -C /tmp; \
    install -Dm755 /tmp/amagi /out/amagi; \
    fetch_release_asset "https://github.com/${TDLR_REPO_OWNER}/${TDLR_REPO_NAME}/releases/download/${TDLR_VERSION}/tdlr-${arch}-unknown-linux-musl.tar.gz" /tmp/tdlr.tar.gz; \
    tar -xzf /tmp/tdlr.tar.gz -C /tmp; \
    install -Dm755 /tmp/tdlr /out/tdlr

FROM alpine:3.21 AS runtime

ENV DHTGBOT_HOME=/var/lib/dhtgbot
ENV RUST_LOG=info
ENV PATH=/opt/dhtgbot/bin:/usr/local/bin:${PATH}

RUN apk add --no-cache aria2 ca-certificates tini \
    && addgroup -S dhtgbot \
    && adduser -S -G dhtgbot -h /var/lib/dhtgbot dhtgbot \
    && install -d -o dhtgbot -g dhtgbot /opt/dhtgbot/bin /opt/dhtgbot/docker /var/lib/dhtgbot

COPY --from=deps /out/dhtgbot /opt/dhtgbot/bin/dhtgbot
COPY --from=deps /out/amagi /usr/local/bin/amagi
COPY --from=deps /out/tdlr /usr/local/bin/tdlr
COPY config.example.yaml /opt/dhtgbot/config.example.yaml
COPY config.example.docker.yaml /opt/dhtgbot/config.example.docker.yaml
COPY docker/entrypoint.sh /opt/dhtgbot/docker/entrypoint.sh

RUN chmod 755 /opt/dhtgbot/bin/dhtgbot /usr/local/bin/amagi /usr/local/bin/tdlr /opt/dhtgbot/docker/entrypoint.sh \
    && chown -R dhtgbot:dhtgbot /opt/dhtgbot /var/lib/dhtgbot

WORKDIR /var/lib/dhtgbot

VOLUME ["/var/lib/dhtgbot"]
EXPOSE 4567 8787 6800

ENTRYPOINT ["/sbin/tini", "--", "/opt/dhtgbot/docker/entrypoint.sh"]
CMD ["dhtgbot"]
