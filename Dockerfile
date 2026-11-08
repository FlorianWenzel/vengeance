# syntax=docker/dockerfile:1
# The UI and the WASM renderer are platform-independent, and Go cross-compiles, so every
# build stage runs on the build machine's platform; only the final image is per-platform.

FROM --platform=$BUILDPLATFORM rust:1.99-alpine AS viz
ARG WASM_BINDGEN_VERSION=0.2.129
RUN apk add --no-cache musl-dev curl bash \
 && rustup target add wasm32-unknown-unknown \
 && arch=$(uname -m) \
 && cd /tmp \
 && f="wasm-bindgen-${WASM_BINDGEN_VERSION}-${arch}-unknown-linux-musl.tar.gz" \
 && curl -fsSLO "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/$f" \
 && echo "$(curl -fsSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/$f.sha256sum" | cut -d' ' -f1)  $f" | sha256sum -c - \
 && tar -xzf wasm-bindgen-*.tar.gz && install -m 0755 wasm-bindgen-*/wasm-bindgen /usr/local/bin/ && rm -rf /tmp/wasm-bindgen-*
WORKDIR /src
COPY viz ./viz
COPY scripts/build-viz.sh ./scripts/build-viz.sh
RUN mkdir -p web/src/viz && bash scripts/build-viz.sh

FROM --platform=$BUILDPLATFORM node:24-alpine AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
COPY --from=viz /src/web/src/viz/pkg ./src/viz/pkg
RUN npm run build

FROM --platform=$BUILDPLATFORM golang:1.26-alpine AS build
ARG VERSION=dev
ARG TARGETOS
ARG TARGETARCH
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
COPY --from=web /src/web/dist ./web/dist
RUN CGO_ENABLED=0 GOOS=$TARGETOS GOARCH=$TARGETARCH go build -trimpath -ldflags "-s -w -X main.version=${VERSION}" -o /out/vengeance ./cmd/vengeance

FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=build /out/vengeance /usr/local/bin/vengeance
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD ["/usr/local/bin/vengeance", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/vengeance"]
CMD ["serve"]
