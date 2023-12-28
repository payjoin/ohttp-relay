FROM debian:bullseye-slim as builder

# Install necessary dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    libc-dev \
    make \
    openssl \
    libpcre3-dev \
    perl \
    zlib1g-dev \
    libssl-dev \
    wget \
    patch 

# Download sources
RUN wget https://openresty.org/download/openresty-1.21.4.1.tar.gz \
    && tar -xzvf openresty-1.21.4.1.tar.gz \
    && wget https://github.com/chobits/ngx_http_proxy_connect_module/archive/refs/tags/v0.0.5.tar.gz \
    && tar -xzvf v0.0.5.tar.gz

# Compile OpenResty with ngx_http_proxy_connect_module
RUN cd /openresty-1.21.4.1 \
    && ./configure --add-module=../ngx_http_proxy_connect_module-0.0.5 \
    && patch -d build/nginx-1.21.4/ -p 1 < ../ngx_http_proxy_connect_module-0.0.5/patch/proxy_connect_rewrite_102101.patch \
    && make && make install

# Final stage
FROM debian:bullseye-slim

# Copy compiled binary
COPY --from=builder /usr/local/openresty /usr/local/openresty

# Copy the Nginx config
COPY default.conf.template /etc/nginx/conf.d/default.conf.template

EXPOSE 80

# Copy the start script
COPY start.sh /start.sh

# Set the start script as the entrypoint
ENTRYPOINT ["/start.sh"]
