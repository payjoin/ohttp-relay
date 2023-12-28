# Payjoin OHTTP Relay

Relays [Oblivious HTTP](https://ietf-wg-ohai.github.io/oblivious-http/draft-ietf-ohai-ohttp.html) requests to protect IP metadata in the [Payjoin v2](https://github.com/bitcoin/bips/pull/1483) protocol. This is based on an [OpenResty](https://openresty.org) NGINX server.

This OHTTP Relay may be generic enough for other applications, too.

## Building the Image

- Building the Docker Image:
- Ensure Docker is installed on your system.
- Clone this repository.
- Navigate to the directory and build the Docker image:

```bash
docker build -t ohttp-relay .
```

## Running the Docker Container for Production

To run the nginx server with automatic HTTPS, execute the following command, replacing your_server_name and your_gateway_url with your actual server name and OHTTP gateway domain:

```bash
docker run -d -p 80:80 -p 443:443 \
  -e SERVER_NAME='your_server_name' \
  -e OHTTP_GATEWAY='your_gateway_domain' \
  --name my-ohttp-relay ohttp-relay
```

## Monitoring and Logs

Monitor the logs of your Docker container to ensure everything is running smoothly:

```bash
docker logs my-ohttp-relay
```

## Updates and Maintenance

- Regularly update your Docker image to get the latest version of Caddy and security updates.
- Use Docker commands to stop, remove, rebuild, and restart the container with the new image.
