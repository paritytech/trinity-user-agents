# The dashboard server, plus what smoke.sh needs: Node.js and npm to install
# the packages, curl and jq, and aapt to read the nightly APK. Both bases are
# pinned by digest; bump a tag and its digest together.
FROM oven/bun:1.4.2-slim@sha256:cb3bbbb08e13a4a2ff400f24c7a2a1d5efa83f6ef8544d52d95a519631e2fc61 AS bun

FROM node:22.23.4-bookworm-slim@sha256:4b150b48b4116e2a22f2c39cc27025fbf4f153e2078fd3b14d159fdb5c7279da

RUN apt-get update \
  && apt-get install -y --no-install-recommends aapt ca-certificates curl jq \
  && rm -rf /var/lib/apt/lists/*
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun

WORKDIR /app
COPY . .
RUN bun build.ts dist

# The chart mounts the state volume at /data owned by root. entrypoint.sh
# hands it to the node user and drops to that user before starting.
ENV PORT=8080 STATE_DIR=/data SITE_DIR=/app/dist HOME=/tmp
EXPOSE 8080
ENTRYPOINT ["/app/entrypoint.sh"]
