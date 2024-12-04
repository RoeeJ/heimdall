FROM debian:bookworm-slim
COPY target/release/heimdall /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/heimdall"] 