# Payjoin OHTTP Relay

Relays [Oblivious HTTP](https://ietf-wg-ohai.github.io/oblivious-http/draft-ietf-ohai-ohttp.html) requests to protect IP metadata in the [Payjoin v2](https://github.com/bitcoin/bips/pull/1483) protocol. This is based on a caddy server.


## Deploying the Image with Proper TLS Support

- Building the Docker Image:
- Ensure Docker is installed on your system.
- Clone the repository containing the Dockerfile and Caddyfile.
- Navigate to the directory and build the Docker image:

```bash

docker build -t caddy-ohttp-relay .
```

## Running the Docker Container for Production

To run the Caddy server with automatic HTTPS, execute the following command, replacing your_server_name and your_gateway_url with your actual server name and OHTTP gateway URL:

```bash

docker run -d -p 80:80 -p 443:443 \
  -e SERVER_NAME='your_server_name' \
  -e OHTTP_GATEWAY='your_gateway_url' \
  --name my-caddy-ohttp-relay caddy-ohttp-relay
```

Caddy will automatically handle HTTPS, including obtaining and renewing SSL/TLS certificates.

## Testing with Staging Environment

- For initial testing, use the staging environment of Let's Encrypt. Modify the Caddyfile to include the tls block as shown below.
- Rebuild the Docker image and run the container.
- Remember to remove the tls block when you are ready to go live.

```conf
tls {
    ca https://acme-staging-v02.api.letsencrypt.org/directory
}
```

## Going Live

- Once you have finished testing, remove the tls block from the Caddyfile to switch back to the production environment of Let's Encrypt.
- Rebuild the Docker image and redeploy the container.
- Caddy will now obtain trusted certificates from Let's Encrypt's production environment.

## Monitoring and Logs

Monitor the logs of your Docker container to ensure everything is running smoothly:

```bash
docker logs my-caddy-ohttp-relay
```

Look out for any errors and confirm that SSL/TLS certificates are being obtained successfully.

## Updates and Maintenance

-  Regularly update your Docker image to get the latest version of Caddy and security updates.
-  Use Docker commands to stop, remove, rebuild, and restart the container with the new image.

