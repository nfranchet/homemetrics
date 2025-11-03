FROM ubuntu:25.04

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && update-ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY target/release/homemetrics /bin/homemetrics

CMD [ "/bin/homemetrics", "--daemon" ]