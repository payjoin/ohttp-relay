# Use the official Caddy image as the base
FROM caddy:2-alpine

# Copy the Caddyfile into the container
COPY Caddyfile /etc/caddy/Caddyfile
