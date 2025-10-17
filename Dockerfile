FROM ubuntu:23.04

COPY target/release/web /bin/homemetrics

CMD [ "/bin/homemetrics" ]