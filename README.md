# OHTTP Relay

A rust implementation of an [Oblivious
HTTP](https://ietf-wg-ohai.github.io/oblivious-http/draft-ietf-ohai-ohttp.html) relay resource.

This work is undergoing active revision in the IETF and so are these
implementations.  Use at your own risk.

## Usage

Run ohttp-relay by setting `PORT` and `GATEWAY_ORIGIN` environment vaiables. For example, to relay from port 3000 to an OHTTP Gateway Resource at `https://payjo.in`, run the following.

```console
PORT=3000 GATEWAY_ORIGIN='https://payjo.in' cargo run
```

Alternatively, set `UNIX_SOCKET` to bind to a unix socket path instead of a TCP port.

This crate is intended to be run behind a reverse proxy like NGINX that can handle TLS for you.
