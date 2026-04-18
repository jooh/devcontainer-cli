FROM alpine:3.20

ARG MESSAGE=unset

RUN mkdir -p /usr/local/etc \
    && printf '%s\n' "$MESSAGE" > /usr/local/etc/acceptance-dockerfile-message
