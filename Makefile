.PHONY: all build test clean docker docker-build docker-push setup

# 配置变量
BINARY_NAME=asr-rs
VERSION=$(shell git describe --tags --always --dirty)
DOCKER_REGISTRY=bean
DOCKER_IMAGE=$(DOCKER_REGISTRY)/$(BINARY_NAME)
PLATFORMS=linux/amd64,linux/arm64

# 检测操作系统和架构
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

# 根据平台设置 whisper 特性
ifeq ($(UNAME_S),Darwin)
    WHISPER_FEATURES=metal
else
    WHISPER_FEATURES=cuda
endif

# 默认目标
all: build

# 开发构建
build-app:
	cargo build --release --features $(WHISPER_FEATURES)

# 测试
test:
	cargo test

# 清理
clean:
	cargo clean
	rm -rf target/

# 安装依赖
setup:
	# 添加目标平台
	rustup target add x86_64-unknown-linux-musl
	rustup target add aarch64-unknown-linux-musl
	# macOS
	brew install FiloSottile/musl-cross/musl-cross

# Linux 构建 (amd64)
build-amd64:
	RUSTFLAGS='-C linker=x86_64-linux-musl-gcc' \
	CC_x86_64_unknown_linux_musl=x86_64-linux-musl-gcc \
	CXX_x86_64_unknown_linux_musl=x86_64-linux-musl-g++ \
	AR_x86_64_unknown_linux_musl=x86_64-linux-musl-ar \
	cargo build --release --target x86_64-unknown-linux-musl \
		--no-default-features \
		--features "reqwest/rustls-tls,cuda"

# Linux 构建 (arm64)
build-arm64:
	RUSTFLAGS='-C linker=aarch64-linux-musl-gcc' \
	CC_aarch64_unknown_linux_musl=aarch64-linux-musl-gcc \
	CXX_aarch64_unknown_linux_musl=aarch64-linux-musl-g++ \
	AR_aarch64_unknown_linux_musl=aarch64-linux-musl-ar \
	cargo build --release --target aarch64-unknown-linux-musl \
		--no-default-features \
		--features "reqwest/rustls-tls,metal"

# Docker 构建
build: setup build-amd64 build-arm64
	docker buildx create --use --name multiarch-builder || true
	docker buildx build --platform $(PLATFORMS) \
		-t $(DOCKER_IMAGE):$(VERSION) \
		-t $(DOCKER_IMAGE):latest \
		--push .

# Docker 本地构建（仅当前平台）
build-local: setup
ifeq ($(UNAME_M),arm64)
	$(MAKE) build-arm64
	docker buildx build --platform linux/arm64 \
		-t $(DOCKER_IMAGE):$(VERSION) \
		-t $(DOCKER_IMAGE):latest \
		--load .
else
	$(MAKE) build-amd64
	docker buildx build --platform linux/amd64 \
		-t $(DOCKER_IMAGE):$(VERSION) \
		-t $(DOCKER_IMAGE):latest \
		--load .
endif

# Docker 推送
push:
	docker push $(DOCKER_IMAGE):$(VERSION)
	docker push $(DOCKER_IMAGE):latest