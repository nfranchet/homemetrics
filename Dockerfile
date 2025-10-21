FROM ubuntu:25.04

COPY target/release/homemetrics /bin/homemetrics

CMD [ "/bin/homemetrics" ]