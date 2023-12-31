#!/bin/sh

# Check if the required environment variables are set
if [ -z "$OHTTP_GATEWAY" ] || [ -z "$SERVER_NAME" ]; then
    echo "Required environment variables are not set. Exiting."
    exit 1
fi

# Directory where cert and key is
SSL_DIR="/etc/nginx/ssl"
mkdir -p ${SSL_DIR}

# Environment variables for certificate and key paths
SSL_CERT_PATH="${SSL_DIR}/nginx.crt"
SSL_KEY_PATH="${SSL_DIR}/nginx.key"

# Generate a self-signed certificate if no certificate exists and no paths are provided.
if [ ! -f "${SSL_CERT_PATH}" ] || [ ! -f "${SSL_KEY_PATH}" ]; then
    echo "No SSL certificates found. Generating self-signed certificates..."
    openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
    -keyout "${SSL_KEY_PATH}" \
    -out "${SSL_CERT_PATH}" \
    -subj "/CN=localhost"
else
    echo "Using provided SSL certificates."
fi

# Substitute environment variables in nginx.conf
envsubst '${OHTTP_GATEWAY},${SERVER_NAME}' < /usr/local/openresty/nginx/conf/nginx.conf.template > /usr/local/openresty/nginx/conf/nginx.conf

# Start Nginx in the foreground
openresty -g 'daemon off;'

