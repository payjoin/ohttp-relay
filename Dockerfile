FROM openresty/openresty:latest as builder

# Install necessary dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    libc-dev \
    make \
    openssl \
    libpcre3-dev \
    zlib1g-dev \
    libssl-dev \
    wget \
    patch \
    lua5.1-dev

# Download sources
RUN wget https://nginx.org/download/nginx-1.25.3.tar.gz \
    && tar -xzvf nginx-1.25.3.tar.gz \
#    && wget https://github.com/openresty/headers-more-nginx-module/archive/v0.36.tar.gz \
#    && tar -xzvf v0.36.tar.gz \
    && wget https://github.com/openresty/lua-nginx-module/archive/refs/tags/v0.10.25.tar.gz \
    && tar -xzvf v0.10.25.tar.gz \
    && wget https://github.com/chobits/ngx_http_proxy_connect_module/archive/refs/tags/v0.0.5.tar.gz \
    && tar -xzvf v0.0.5.tar.gz

# Compile Nginx with headers-more-nginx-module
RUN cd /nginx-1.25.3 \
    && patch -p1 < ../ngx_http_proxy_connect_module-0.0.5/patch/proxy_connect_rewrite_102101.patch \
    && ./configure --with-compat --add-dynamic-module=../lua-nginx-module-0.10.25 --add-dynamic-module=../ngx_http_proxy_connect_module-0.0.5 \
    && make modules

# Final stage
FROM nginx:latest

# Copy compiled modules
#COPY --from=builder /nginx-1.25.3/objs/ngx_http_headers_more_filter_module.so /etc/nginx/modules/
COPY --from=builder /nginx-1.25.3/objs/ngx_http_lua_module.so /etc/nginx/modules/
COPY --from=builder /nginx-1.25.3/objs/ngx_http_proxy_connect_module.so /etc/nginx/modules/

# Copy the Nginx config
COPY nginx.template.conf /etc/nginx/nginx.template.conf

EXPOSE 80

# Copy the start script
COPY start.sh /start.sh

# Set the start script as the entrypoint
ENTRYPOINT ["/start.sh"]
