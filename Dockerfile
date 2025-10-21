FROM ubuntu:23.04

COPY target/release/homemetrics /bin/homemetrics

CMD [ "/bin/homemetrics" ]