FROM alpine:3.21
RUN apk add --no-cache rust cargo alpine-sdk openssl-dev
COPY . /build
WORKDIR /build
RUN ["cargo", "build", "--release"]
EXPOSE 3000
ENTRYPOINT ["cargo", "run", "--release"]
