FROM frolvlad/alpine-glibc:alpine-3.9_glibc-2.29

WORKDIR /app
COPY target/release/agent /app/agent

ENTRYPOINT [ "/app/agent" ]