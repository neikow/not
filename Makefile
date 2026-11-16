# Dev shortcuts. `make` lists them.

SHELL := /bin/bash
.DEFAULT_GOAL := help

export JAVA_HOME ?= /Applications/Android Studio.app/Contents/jbr/Contents/Home
export ANDROID_HOME ?= $(HOME)/Library/Android/sdk

# Server URL that pairing codes/QR codes point devices at, e.g. make pair URL=https://not.example.ts.net
URL ?=
# Port for `make serve` (8080 is taken by the docker server).
PORT ?= 8081
# First attached non-emulator device (the phone over wireless adb); override with PHONE=<serial>.
PHONE ?= $(shell adb devices 2>/dev/null | awk 'NR>1 && $$2=="device" && $$1 !~ /^emulator/ {print $$1; exit}')
EMULATOR ?= emulator-5554

RUST_SRC := $(shell find crates -name '*.rs' -o -name Cargo.toml) Cargo.toml Cargo.lock
APPLE_CORE := apple/NotCore/Sources/NotCore/Generated.swift
ANDROID_CORE := android/app/src/main/jniLibs/arm64-v8a/libnot_ffi.so
MAC_APP := build/xcode/Build/Products/Debug/not.app

.PHONY: help server server-down server-logs pair devices serve serve-pair \
	mac mac-run android android-test release-apk phone emu test lint fmt coverage check clean

help: ## List shortcuts
	@awk 'BEGIN {FS = ":.*## "} /^[a-z-]+:.*## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ---- server ----

server: ## Build and start the docker server on :8080
	docker compose up -d --build

server-down: ## Stop the docker server (data volume is kept)
	docker compose down

server-logs: ## Follow docker server logs
	docker compose logs -f not

pair: ## One-time pairing code + QR from the docker server (URL=...)
	docker compose exec not not-server pair $(URL)

devices: ## List devices paired with the docker server
	docker compose exec not not-server devices

serve: ## Run the server natively on :8081 (PORT=...) with build/dev.db
	@mkdir -p build
	NOT_DB=build/dev.db NOT_ADDR=0.0.0.0:$(PORT) cargo run -p not-server

serve-pair: ## Pairing code for `make serve` (URL defaults to this Mac's LAN IP)
	NOT_DB=build/dev.db cargo run -q -p not-server -- pair $(or $(URL),http://$(shell ipconfig getifaddr en0):$(PORT))

# ---- macOS ----

$(APPLE_CORE): $(RUST_SRC)
	./scripts/build-apple.sh

mac: $(APPLE_CORE) ## Build the Mac app (rebuilds the Rust core when it changed)
	cd apple && xcodegen generate --quiet
	xcodebuild -project apple/Not.xcodeproj -scheme NotMac -configuration Debug \
		-destination platform=macOS,arch=arm64 -derivedDataPath build/xcode -quiet build

mac-run: mac ## Build, then (re)start the Mac app
	-pkill -x not && sleep 0.5
	open $(MAC_APP)

# ---- Android ----

$(ANDROID_CORE): $(RUST_SRC)
	./scripts/build-android.sh

android: $(ANDROID_CORE) ## Build the debug APK (rebuilds the Rust core when it changed)
	cd android && ./gradlew -q assembleDebug

android-test: $(ANDROID_CORE) ## Android JVM unit tests
	cd android && ./gradlew -q testDebugUnitTest

release-apk: $(ANDROID_CORE) ## Signed release APK, with keys from ~/.android/not-release.env (CI does this on main)
	source ~/.android/not-release.env && cd android && ./gradlew -q assembleRelease
	@echo android/app/build/outputs/apk/release/app-release.apk

phone: $(ANDROID_CORE) ## Install on the phone and launch (PHONE=<serial> to pick one)
	@test -n "$(PHONE)" || { echo "no phone attached: adb pair/connect it first"; exit 1; }
	cd android && ANDROID_SERIAL=$(PHONE) ./gradlew -q installDebug
	adb -s $(PHONE) shell am start -n dev.lysen.not/.MainActivity

emu: $(ANDROID_CORE) ## Install on the emulator and launch
	cd android && ANDROID_SERIAL=$(EMULATOR) ./gradlew -q installDebug
	adb -s $(EMULATOR) shell am start -n dev.lysen.not/.MainActivity

# ---- Rust ----

test: ## Run Rust tests
	cargo test --workspace --all-features

lint: ## Clippy over the workspace
	cargo clippy --workspace --all-targets -- -D warnings

fmt: ## Format Rust code
	cargo fmt --all

coverage: ## Rust line coverage per file (needs `cargo install cargo-llvm-cov`)
	cargo llvm-cov --workspace --all-features --summary-only

check: ## Everything CI checks: fmt, clippy, Rust tests, Android unit tests
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
	cargo test --workspace --all-features --locked
	$(MAKE) android-test

clean: ## Remove generated bindings and app builds (keeps target/ and build/dev.db)
	rm -rf build/apple build/xcode apple/NotCore/NotFFI.xcframework $(APPLE_CORE) \
		android/app/src/main/jniLibs android/app/src/main/java/dev/lysen/not/core android/app/build
