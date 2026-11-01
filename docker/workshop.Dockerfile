# officina-workshop:local (AD-99): the base every workshop worker runs on.
# Built by Officina's Build images from a context it writes fresh: this
# file, officina-entry, and ca/ with zero or one officina-ca.crt.
FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4 AS base
# hadolint ignore=DL3008
RUN apt-get update \
 && apt-get install -y --no-install-recommends bash ca-certificates coreutils curl git python-is-python3 python3 \
 && rm -rf /var/lib/apt/lists/*
# The corporate root, when the floor names one, before any HTTPS download
# (D-612), so every later step trusts an inspecting proxy.
COPY ca/ /usr/local/share/ca-certificates/
RUN update-ca-certificates

# Claude Code by its documented installer at the host's version, which
# checks the binary against the release manifest over TLS (the gate 1
# entry); the final stage copies the binary alone.
FROM base AS claude
ARG CLAUDE_CODE_VERSION
ENV CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
RUN useradd -m -u 1000 builder
USER builder
WORKDIR /home/builder
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN curl -fsSL https://claude.ai/install.sh -o install.sh \
 && bash install.sh "${CLAUDE_CODE_VERSION}" \
 && cp -L /home/builder/.local/bin/claude /home/builder/claude-bin

FROM base
ARG UV_VERSION=0.13.6
ARG UV_SHA256=f29c3e94ffabd7e6f3ba521dba169a9b7ec12100bf3257087f0f24f6123f3645
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN curl -fsSL -o /tmp/uv.tar.gz "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-x86_64-unknown-linux-gnu.tar.gz" \
 && echo "${UV_SHA256}  /tmp/uv.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/uv.tar.gz -C /usr/local/bin --strip-components=1 \
 && rm /tmp/uv.tar.gz
COPY --from=claude --chown=0:0 --chmod=0755 /home/builder/claude-bin /opt/officina/bin/claude
COPY --chown=0:0 --chmod=0755 officina-entry /opt/officina/bin/officina-entry
RUN useradd -m -u 1000 -s /bin/bash worker \
 && mkdir -p /work/.officina/in /work/.officina/out /work/.officina/home /work/.officina/config /work/.officina/mounts /work/.officina/login \
 && chown -R worker:worker /work \
 && (find / -xdev -perm /6000 -type f -exec chmod a-s {} + || true)
ENV PATH=/opt/officina/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt \
    NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt \
    UV_NATIVE_TLS=1
WORKDIR /work
USER worker
ENTRYPOINT ["/opt/officina/bin/officina-entry"]
