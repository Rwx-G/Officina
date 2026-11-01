# officina-gateway:local (AD-99, AD-103): the static gateway alone, no
# shell, no certificate, no resolver configuration of its own.
FROM scratch
COPY --chown=0:0 --chmod=0755 officina-gateway /officina-gateway
USER 65534:65534
ENTRYPOINT ["/officina-gateway"]
