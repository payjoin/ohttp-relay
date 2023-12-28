#!/bin/sh

# Check if the required environment variables are set
if [ -z "$OHTTP_GATEWAY" ] || [ -z "$SERVER_NAME" ]; then
    echo "Required environment variables are not set. Exiting."
    exit 1
fi

# Substitute environment variables in nginx.conf
envsubst '${OHTTP_GATEWAY},${SERVER_NAME}' < /etc/nginx/conf.d/default.conf.template > /etc/nginx/conf.d/default.conf

# Start Nginx in the foreground
nginx -g 'daemon off;'

