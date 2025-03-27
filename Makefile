# Image configuration
DOCKER_REGISTRY ?= ghcr.io
DOCKER_REPO ?= mcp-gw/mcp-gw
IMAGE_NAME ?= gw
VERSION ?= $(shell git describe --tags --always --dirty)
IMAGE_TAG ?= $(VERSION)
IMAGE_FULL_NAME ?= $(DOCKER_REGISTRY)/$(DOCKER_REPO)/$(IMAGE_NAME):$(IMAGE_TAG)
DOCKER_BUILDER ?= docker
DOCKER_BUILD_ARGS ?=
KIND_CLUSTER_NAME ?= mcp-gw

# docker
.PHONY: docker
docker:
	$(DOCKER_BUILDER) build $(DOCKER_BUILD_ARGS) -t $(IMAGE_FULL_NAME) .

# build
.PHONY: build
build:
	cargo build --release

# lint
.PHONY: lint
lint:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings

# test
.PHONY: test
test:
	cargo test --all-targets --all-features
