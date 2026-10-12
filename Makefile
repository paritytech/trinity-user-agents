# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check check-generated clean playground wasm wasm-crypto-test uniffi uniffi-kotlin android-check provider-android-check ios-build ios-run ios-chat-run ios-chat-host-playground-run ios-chat-all android-jni android-publish-local dotli-link dev dev-cli dev-bootstrap debugger dev-link-check e2e-dotli e2e-cli-diagnosis e2e-signing-cli e2e-pairing-cli e2e-chat-cli e2e-pocket-cli e2e-cross-product-storage e2e-cross-product-ringvrf e2e-cross-product-signing e2e-cli-update headless install cli-runner cli-dist matrix explorer xcframework

CARGO ?= cargo
# The dated nightly CI runs; see nightly-toolchain.
NIGHTLY_TOOLCHAIN ?= $(or $(TRUAPI_NIGHTLY_TOOLCHAIN),$(shell head -n1 nightly-toolchain))
TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
JS_PACKAGES := js/packages
EXPLORER := explorer
DOTLI := hosts/dotli
HOST_WASM_PKG := $(JS_PACKAGES)/truapi-host
PROVIDER_WASM_PKG := $(JS_PACKAGES)/truapi-provider
HOST_CALLBACKS_GENERATED := $(HOST_WASM_PKG)/src/generated/host-callbacks.ts
HOST_WASM_ADAPTER_GENERATED := $(HOST_WASM_PKG)/src/generated/host-callbacks-adapter.ts
HOST_WASM_WORKER_CALLBACKS_GENERATED := $(HOST_WASM_PKG)/src/generated/worker-callbacks.ts
HOST_WASM_WEB := $(HOST_WASM_PKG)/dist/wasm/web/truapi_server.js
HOST_WASM_WEB_BINARY := $(HOST_WASM_PKG)/dist/wasm/web/truapi_server_bg.wasm
DOTLI_HOST_VITE_CONFIG := $(DOTLI)/apps/host/vite.config.ts
DOTLI_UI := $(DOTLI)/packages/ui
DOTLI_NODE_MODULES := $(DOTLI)/node_modules
DOTLI_TRUAPI_LINK := $(DOTLI_NODE_MODULES)/@parity/truapi
DOTLI_HOST_WASM_LINK := $(DOTLI_NODE_MODULES)/@parity/truapi-host
DOTLI_UI_TRUAPI_SHADOW := $(DOTLI_UI)/node_modules/@parity/truapi
DOTLI_UI_HOST_WASM_SHADOW := $(DOTLI_UI)/node_modules/@parity/truapi-host
DEBUGGER_PKG := $(JS_PACKAGES)/truapi-debugger
DEBUGGER_PORT ?= 9231
VITE_NETWORKS ?= paseo-next-v2,previewnet
export VITE_NETWORKS

# Local product URLs (`http://localhost:5173/localhost:3000`) are intentionally
# gated behind dotli's debug build flag, so the dev target must run the debug
# preview by default. Override with `DOTLI_PREVIEW=preview` to test production
# preview behavior.
DOTLI_PREVIEW ?= preview:debug

# The truapi runtime declares these modules unconditionally, so the crate does
# not parse without them. They are gitignored and produced by scripts/codegen.sh.
GENERATED_RUST := \
	rust/crates/truapi/src/generated/mod.rs \
	rust/crates/truapi/src/generated/dispatcher.rs \
	rust/crates/truapi/src/generated/wire_table.rs \
	rust/crates/truapi/src/wasm/generated_bridge.rs

check-generated:
	@for file in $(GENERATED_RUST); do \
		test -f "$$file" || { echo "Missing $$file. Run: make codegen"; exit 1; }; \
	done

help: ## Show this help.
	@awk 'BEGIN { FS = ":.*##"; printf "Usage: make <target>\n\nTargets:\n" } \
	      /^[a-zA-Z0-9_-]+:.*?##/ { printf "  %-12s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

setup: ## First-time setup: submodules, JS dependencies, generated artifacts.
	git submodule update --init --recursive
	# --ignore-scripts: the workspace `prepare` builds need generated sources
	# that only exist after codegen.sh, which also builds the packages.
	npm ci --ignore-scripts
	./scripts/codegen.sh
	$(MAKE) uniffi
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile
	$(MAKE) dotli-link

build: check-generated ## Build the Rust workspace and the TypeScript client.
	cargo build --workspace
	cd $(TRUAPI_PKG) && npm run build
	cd $(HOST_WASM_PKG) && npm run build

headless: ## Build the truapi-host CLI and generated TypeScript client.
	@[ -x node_modules/.bin/tsc ] && [ -x node_modules/.bin/prettier ] \
		|| npm ci --ignore-scripts
	./scripts/codegen.sh
	cargo build -p truapi-host-cli
	bun scripts/build-cli-runner.ts "$(CLI_DIST_DIR)"

install: headless ## Install the truapi-host CLI into Cargo's bin dir; use as `make headless install`.
	# A prebuilt install and a cargo one shadow each other depending on PATH
	# order, so clear the prebuilt one before taking over.
	bash scripts/truapi-host-installer.sh --uninstall
	cargo install --path rust/crates/truapi-host-cli --bin truapi-host --locked --force
	@echo
	@echo "Installed a local build of truapi-host. It does not auto-update."
	@echo "To go back to the prebuilt release:"
	@echo "  curl -fsSL $(CLI_INSTALLER_URL) | bash"

# Release packaging for the truapi-host binary. CLI_TARGET picks the triple;
# CLI_VERSION defaults to the crate version, which tracks the protocol version.
# The layout here is what scripts/truapi-host-installer.sh expects to download.
CLI_INSTALLER_URL := https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh
CLI_DIST_DIR := target/dist
# Default to the triple that is actually published, not the rustc host: the
# Linux releases are musl so one artifact per architecture runs anywhere.
CLI_TARGET ?= $(shell rustc -vV | sed -n 's/^host: //p' | sed 's/-linux-gnu$$/-linux-musl/')
CLI_VERSION ?= $(shell awk -F'"' '/^version = /{print $$2; exit}' rust/crates/truapi-host-cli/Cargo.toml)
CLI_ARCHIVE = truapi-host-$(CLI_VERSION)-$(CLI_TARGET).tar.gz
CLI_RUNNER := $(CLI_DIST_DIR)/runner.js
CLI_SCRIPT_TYPES_SOURCE := rust/crates/truapi-host-cli/js/script-types.d.ts
CLI_SCRIPT_TYPES := $(CLI_DIST_DIR)/script-types.d.ts
CLI_CONTAINER := $(CLI_DIST_DIR)/sandbox-assets/container.js
CLI_STAGE = $(CLI_DIST_DIR)/$(CLI_TARGET)
# macOS ships shasum, most Linux images ship only sha256sum.
SHA256 := $(shell command -v sha256sum >/dev/null 2>&1 && echo "sha256sum" || echo "shasum -a 256")

# Generated SDK sources are needed before bundling. CI builds the runner,
# dev container once, then reuses them in every target archive.
$(CLI_RUNNER): $(CLI_CONTAINER)
	@test -f "$@" || bun scripts/build-cli-runner.ts "$(CLI_DIST_DIR)"

$(CLI_SCRIPT_TYPES): $(CLI_SCRIPT_TYPES_SOURCE)
	mkdir -p $(CLI_DIST_DIR)
	cp $< $@

$(CLI_CONTAINER):
	bun scripts/build-cli-runner.ts "$(CLI_DIST_DIR)"

cli-runner: $(CLI_SCRIPT_TYPES) ## Bundle the product-script runner, browser sandbox, and script types into target/dist.
	bun scripts/build-cli-runner.ts "$(CLI_DIST_DIR)"
	node scripts/check-host-script-types.mjs

cli-dist: check-generated $(CLI_RUNNER) $(CLI_SCRIPT_TYPES) ## Package truapi-host for CLI_TARGET into target/dist in the release artifact layout.
	rustup target add $(CLI_TARGET)
	$(CARGO) build -p truapi-host-cli --release --target $(CLI_TARGET)
	rm -rf $(CLI_STAGE)
	mkdir -p $(CLI_STAGE)
	cp target/$(CLI_TARGET)/release/truapi-host $(CLI_RUNNER) $(CLI_SCRIPT_TYPES) $(CLI_STAGE)/
	cp -R $(CLI_DIST_DIR)/sandbox-assets $(CLI_STAGE)/
	tar -czf $(CLI_DIST_DIR)/$(CLI_ARCHIVE) -C $(CLI_STAGE) truapi-host runner.js script-types.d.ts sandbox-assets
	cd $(CLI_DIST_DIR) && $(SHA256) $(CLI_ARCHIVE) > $(CLI_ARCHIVE).sha256
	@echo "packaged $(CLI_DIST_DIR)/$(CLI_ARCHIVE)"

codegen: ## Regenerate generated TS/Rust artifacts from the Rust crates.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

wasm: check-generated ## Rebuild the truapi runtime and truapi-provider WASM bundles under js/packages/*/dist/.
	cd $(HOST_WASM_PKG) && npm run build:wasm
	cd $(PROVIDER_WASM_PKG) && npm run build

wasm-crypto-test: ## Run crypto/vector tests on wasm32 via wasm-pack/node.
	wasm-pack test --node rust/crates/truapi --test wasm_crypto_vectors --no-default-features --features runtime

dotli-link: ## Link dotli to this checkout's local @parity/truapi packages.
	cd $(DOTLI) && TRUAPI_REPO="$(CURDIR)" bun run link:truapi

# uniffi-bindgen scans the cdylib's metadata symbols, which `release` strips, so
# codegen builds use the unstripped `codegen` profile (see [profile.codegen]).
UNIFFI_CDYLIB_DIR := target/codegen
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
UNIFFI_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi.dylib
PROVIDER_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi_provider.dylib
else
UNIFFI_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi.so
PROVIDER_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi_provider.so
endif

UNIFFI_SWIFT_TMP := target/uniffi-swift-out
PROVIDER_SWIFT_TMP := target/uniffi-provider-swift-out

uniffi: check-generated ## Generate Swift bindings from the truapi cdylib into target/uniffi-swift-out (consumed by ios/truapi-host/scripts/rebuild.sh).
	$(CARGO) build -p truapi --profile codegen
	rm -rf $(UNIFFI_SWIFT_TMP)
	mkdir -p $(UNIFFI_SWIFT_TMP)
	$(CARGO) run -p uniffi-bindgen-cli -- generate \
		--library $(UNIFFI_CDYLIB) \
		--language swift \
		--out-dir $(UNIFFI_SWIFT_TMP)

IOS_HOST ?= ../polkadot-app-ios-v2
IOS_DERIVED_DATA ?= $(IOS_HOST)/build/DerivedData
IOS_CONFIGURATION ?= Debug
IOS_SWIFT_FLAGS ?= -DNIGHTLY -DW3S -DIOS_PASEO_E2E -DTRUAPI_RUNTIME_DEFAULT
IOS_SIMULATOR_DEVICE ?=
IOS_XCODE_DESTINATION ?= generic/platform=iOS Simulator
IOS_BUNDLE ?= io.parity.polkadotapp.develop
IOS_GOOGLE_SERVICE_PLIST ?= $(IOS_HOST)/polkadot-app/GoogleService/GoogleService-Info-Release.plist
IOS_PRODUCT_HOST ?= truapi-playground.dot
IOS_PRODUCT_URL ?= http://localhost:3100
IOS_CHAT_PRODUCT_DIR ?= playground
IOS_CHAT_PRODUCT_HOST ?= truapi-playground.dot
IOS_CHAT_PRODUCT_NAME ?= TrUAPI Playground
IOS_CHAT_PRODUCT_URL ?= http://127.0.0.1:3100
IOS_HOST_PLAYGROUND_DIR ?= ../host-playground
IOS_HOST_PLAYGROUND_HOST ?= host-playground.dot
IOS_HOST_PLAYGROUND_NAME ?= Host Playground
IOS_HOST_PLAYGROUND_URL ?= http://127.0.0.1:3101
IOS_APP := $(abspath $(IOS_DERIVED_DATA)/Build/Products/$(IOS_CONFIGURATION)-iphonesimulator/polkadot-app.app)

ios-bootstrap: ## Generate everything hosts/ios needs before Xcode can load its package graph (SIM_ONLY=1 for simulator slices only).
	./scripts/codegen.sh
	./ios/truapi-host/scripts/rebuild.sh
	$(MAKE) provider-swift
	sh ios/truapi-provider/scripts/sync-bindings.sh
	@echo "hosts/ios can now be opened in Xcode."

ios-build: ## Rebuild the local Rust package and the TestFlight-configured iOS simulator app.
	@test -d "$(IOS_HOST)/.git" || { \
		echo "Missing iOS checkout at $(IOS_HOST); set IOS_HOST to polkadot-app-ios-v2"; \
		exit 1; \
	}
	./ios/truapi-host/scripts/rebuild.sh
	cd $(IOS_HOST) && \
		TRUAPI_LOCAL_PATH="$(CURDIR)" \
		TRUAPI_USE_LOCAL_BINARY=1 \
		RUN_IN_CI=true xcodebuild \
		-project polkadot-app.xcodeproj \
		-scheme polkadot-app \
		-configuration $(IOS_CONFIGURATION) \
		-destination '$(IOS_XCODE_DESTINATION)' \
		-derivedDataPath $(abspath $(IOS_DERIVED_DATA)) \
		ARCHS=arm64 \
		ONLY_ACTIVE_ARCH=YES \
		BASE_SWIFT_FLAGS='$(IOS_SWIFT_FLAGS)' \
		clean build
	cp "$(IOS_GOOGLE_SERVICE_PLIST)" "$(IOS_APP)/GoogleService-Info.plist"
	codesign --force --sign - --preserve-metadata=entitlements "$(IOS_APP)"

ios-run: ios-build ## Build and launch the local TrUAPI playground in an iPhone simulator.
	TRUAPI_IOS_E2E_DEVICE="$(IOS_SIMULATOR_DEVICE)" \
	TRUAPI_IOS_E2E_APP="$(IOS_APP)" \
	TRUAPI_IOS_E2E_BUNDLE="$(IOS_BUNDLE)" \
	TRUAPI_IOS_E2E_PRODUCT_HOST="$(IOS_PRODUCT_HOST)" \
	TRUAPI_IOS_E2E_PRODUCT_URL="$(IOS_PRODUCT_URL)" \
	node scripts/launch-ios-playground.mjs

ios-chat-run: ios-build ## Run the TrUAPI Playground Chat diagnosis in an iPhone simulator.
	TRUAPI_IOS_E2E_DEVICE="$(IOS_SIMULATOR_DEVICE)" \
	TRUAPI_IOS_E2E_APP="$(IOS_APP)" \
	TRUAPI_IOS_E2E_BUNDLE="$(IOS_BUNDLE)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_DIR="$(IOS_CHAT_PRODUCT_DIR)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST="$(IOS_CHAT_PRODUCT_HOST)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME="$(IOS_CHAT_PRODUCT_NAME)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_URL="$(IOS_CHAT_PRODUCT_URL)" \
	node scripts/launch-ios-chat-playground.mjs

ios-chat-host-playground-run: ios-build ## Verify Host Playground Chat through the workspace-linked TrUAPI client.
	TRUAPI_IOS_E2E_DEVICE="$(IOS_SIMULATOR_DEVICE)" \
	TRUAPI_IOS_E2E_APP="$(IOS_APP)" \
	TRUAPI_IOS_E2E_BUNDLE="$(IOS_BUNDLE)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_DIR="$(abspath $(IOS_HOST_PLAYGROUND_DIR))" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST="$(IOS_HOST_PLAYGROUND_HOST)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME="$(IOS_HOST_PLAYGROUND_NAME)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_URL="$(IOS_HOST_PLAYGROUND_URL)" \
	TRUAPI_IOS_E2E_CHAT_ROOM_ID="host-playground-room" \
	TRUAPI_IOS_E2E_CHAT_MESSAGE="!flip" \
	TRUAPI_IOS_E2E_CHAT_EXPECTED_REPLY="Flipping the coin!" \
	TRUAPI_IOS_E2E_CHAT_DIAGNOSIS="0" \
	TRUAPI_IOS_E2E_CHAT_EXPECTED_STARTUP_MESSAGE="" \
	TRUAPI_IOS_E2E_CHAT_EXPECT_CUSTOM_RENDERER="1" \
	TRUAPI_IOS_E2E_CHAT_SCREENSHOT="artifacts/host-playground-coin-flip-chat.png" \
	TRUAPI_IOS_E2E_CHAT_TRUAPI_DIR="$(abspath js/packages/truapi)" \
	node scripts/launch-ios-chat-playground.mjs

ios-chat-all: ios-chat-run ios-chat-host-playground-run ## Run both local iOS Chat playground integrations.

UNIFFI_KOTLIN_OUT := android/truapi-host/src/main/kotlin/generated

uniffi-kotlin: check-generated ## Regenerate Kotlin UniFFI bindings from the truapi cdylib.
	$(CARGO) build -p truapi --profile codegen
	rm -rf $(UNIFFI_KOTLIN_OUT)
	mkdir -p $(UNIFFI_KOTLIN_OUT)
	$(CARGO) run -p uniffi-bindgen-cli -- generate \
		--library $(UNIFFI_CDYLIB) \
		--language kotlin \
		--out-dir $(UNIFFI_KOTLIN_OUT)

# Android ABIs to cross-compile the cdylib for. arm64 + armv7 cover physical
# devices; x86_64 covers the emulator on Intel/Apple-silicon hosts.
ANDROID_ABIS ?= arm64-v8a armeabi-v7a x86_64
ANDROID_JNILIBS := android/truapi-host/src/main/jniLibs

android-jni: check-generated ## Cross-compile libtruapi.so for Android ABIs into jniLibs (needs cargo-ndk + NDK).
	@command -v cargo-ndk >/dev/null || { echo "cargo-ndk not found: cargo install cargo-ndk"; exit 1; }
	$(CARGO) ndk $(foreach abi,$(ANDROID_ABIS),-t $(abi)) \
		-o $(ANDROID_JNILIBS) \
		build --release -p truapi
	# cargo-ndk also copies dependency cdylib intermediates (hash-suffixed,
	# statically linked into libtruapi.so already); keep only ours.
	find $(ANDROID_JNILIBS) -name '*.so' ! -name 'libtruapi.so' -delete

android-check: uniffi-kotlin ## Compile the Kotlin host adapter against freshly generated bindings (needs Gradle + Android SDK).
	gradle :truapi-host:compileReleaseKotlin

android-publish-local: uniffi-kotlin ## Generate Kotlin bindings, then publish the AAR to ~/.m2 as io.parity:truapi-host-android:0.0.0-local (needs Gradle + JDK 17). Run `make android-jni` first to bundle the per-ABI cdylibs into the AAR.
	gradle :truapi-host:publishReleasePublicationToMavenLocal

# truapi-provider ships as its own per-platform artifacts (iOS xcframework,
# Android AAR, npm wasm) so a host consumes chain transport without depending on
# the Rust crate. The `uniffi` feature carries no `ws` backend: these builds are
# the light client alone.
PROVIDER_KOTLIN_OUT := android/truapi-provider/src/main/kotlin/generated
PROVIDER_JNILIBS := android/truapi-provider/src/main/jniLibs

provider-swift: ## Generate the TrUAPIProvider Swift bindings into target/uniffi-provider-swift-out (no Xcode, no iOS targets).
	$(CARGO) build -p truapi-provider --profile codegen --no-default-features --features uniffi
	rm -rf $(PROVIDER_SWIFT_TMP)
	mkdir -p $(PROVIDER_SWIFT_TMP)
	$(CARGO) run -p uniffi-bindgen-cli -- generate \
		--library $(PROVIDER_CDYLIB) \
		--language swift \
		--out-dir $(PROVIDER_SWIFT_TMP)

provider-ios: ## Build the TrUAPIProvider Swift bindings + xcframework (adds --sim-only via SIM_ONLY=1).
	bash ios/truapi-provider/scripts/rebuild.sh $(if $(SIM_ONLY_ON),--sim-only,)

provider-kotlin: ## Regenerate Kotlin UniFFI bindings from the truapi-provider cdylib.
	$(CARGO) build -p truapi-provider --profile codegen --no-default-features --features uniffi
	rm -rf $(PROVIDER_KOTLIN_OUT)
	mkdir -p $(PROVIDER_KOTLIN_OUT)
	$(CARGO) run -p uniffi-bindgen-cli -- generate \
		--library $(PROVIDER_CDYLIB) \
		--language kotlin \
		--out-dir $(PROVIDER_KOTLIN_OUT)

provider-android-jni: ## Cross-compile libtruapi_provider.so for Android ABIs into the module's jniLibs (needs cargo-ndk + NDK).
	@command -v cargo-ndk >/dev/null || { echo "cargo-ndk not found: cargo install cargo-ndk"; exit 1; }
	$(CARGO) ndk $(foreach abi,$(ANDROID_ABIS),-t $(abi)) \
		-o $(PROVIDER_JNILIBS) \
		build --release -p truapi-provider --no-default-features --features uniffi

provider-android-check: provider-kotlin ## Assemble the provider AAR against freshly generated bindings (needs Gradle + Android SDK).
	@test -n "$$(find $(PROVIDER_KOTLIN_OUT) -name '*.kt' -print -quit)" \
		|| { echo "no generated Kotlin under $(PROVIDER_KOTLIN_OUT): the module would compile an empty source set and pass"; exit 1; }
	gradle :truapi-provider:assembleRelease

provider-android-publish-local: provider-kotlin provider-android-jni ## Publish the self-contained provider AAR (bindings + cdylib) to ~/.m2.
	gradle :truapi-provider:publishReleasePublicationToMavenLocal

test: check-generated ## Run Rust + TypeScript client tests.
	cargo test --workspace
	cd $(TRUAPI_PKG) && npm test
	cd $(HOST_WASM_PKG) && npm run build && npm test

check: check-generated ## Full verification suite (build, fmt, clippy, test, TS tests, playground build + lint).
	cargo build --workspace
	cargo check --target wasm32-unknown-unknown -p truapi
	cargo +$(NIGHTLY_TOOLCHAIN) fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --all-features --all-targets
	cd $(TRUAPI_PKG) && npm run build && npm test
	cd $(HOST_WASM_PKG) && npm install --no-fund --no-audit && npm run build && npm test
	cd $(PLAYGROUND) && yarn build && yarn lint && yarn test:unit

clean: ## Remove local build/test artifacts without deleting dependencies.
	cargo clean
	rm -rf \
		$(TRUAPI_PKG)/dist \
		$(TRUAPI_PKG)/tsconfig.tsbuildinfo \
		$(HOST_WASM_PKG)/dist \
		$(HOST_WASM_PKG)/tsconfig.tsbuildinfo \
		$(PLAYGROUND)/.next \
		$(PLAYGROUND)/out \
		$(PLAYGROUND)/test-results \
		$(PLAYGROUND)/tsconfig.tsbuildinfo \
		$(PLAYGROUND)/tests/tsconfig.tsbuildinfo \
		$(DOTLI)/.turbo \
		$(DOTLI)/apps/host/dist \
		$(DOTLI)/apps/protocol/dist \
		$(DOTLI)/apps/sandbox/dist \
		$(DOTLI)/test-results

playground: ## Refresh the playground's @parity/truapi snapshot and rebuild.
	cd $(TRUAPI_PKG) && npm run build
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install
	cd $(PLAYGROUND) && yarn build

dev-bootstrap: ## Prepare ignored generated/build artifacts needed by dotli preview.
	git submodule update --init --recursive
	# --ignore-scripts: the workspace `prepare` builds need generated sources
	# that only exist after codegen.sh, which also builds the packages.
	if [ ! -d node_modules ]; then npm ci --ignore-scripts; fi
	./scripts/codegen.sh
	cd $(HOST_WASM_PKG) && npm run build
	# Release profile, because dotli precaches the WASM in its service worker and
	# vite-plugin-pwa fails the build outright on anything over its workbox limit.
	# A dev-profile build is several times that limit; a release build is well
	# under it. TRUAPI_WASM_PROFILE=dev is therefore not usable with `make dev`
	# or `make e2e-dotli` at all: dev-link-check rejects the artifact rather than
	# letting dotli fail deeper in. Build one directly with
	# `TRUAPI_WASM_PROFILE=dev make wasm` if you need it for something else.
	$(MAKE) wasm
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile
	$(MAKE) dev-link-check

dev-link-check: dotli-link ## Verify dotli can resolve the local @parity/truapi-host package.
	@test -f "$(HOST_CALLBACKS_GENERATED)" || (echo "Missing generated host callbacks. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_ADAPTER_GENERATED)" || (echo "Missing generated host callbacks WASM adapter. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_WORKER_CALLBACKS_GENERATED)" || (echo "Missing generated host callbacks worker bridge. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_PKG)/dist/index.js" || (echo "Missing @parity/truapi-host dist. Run: npm run build --prefix $(HOST_WASM_PKG)"; exit 1)
	@test -f "$(HOST_WASM_WEB)" || (echo "Missing @parity/truapi-host web WASM glue. Run: make wasm"; exit 1)
	@test -f "$(HOST_WASM_WEB_BINARY)" || (echo "Missing @parity/truapi-host web WASM binary. Run: make wasm"; exit 1)
	@node scripts/check-dotli-wasm-precache.mjs "$(HOST_WASM_WEB_BINARY)" "$(DOTLI_HOST_VITE_CONFIG)"
	@test -e "$(DOTLI_TRUAPI_LINK)/package.json" || (echo "dotli cannot resolve @parity/truapi. Run top-level: make dotli-link"; exit 1)
	@test -e "$(DOTLI_HOST_WASM_LINK)/package.json" || (echo "dotli cannot resolve @parity/truapi-host. Run top-level: make dotli-link"; exit 1)
	@test ! -e "$(DOTLI_UI_TRUAPI_SHADOW)/package.json" || (echo "$(DOTLI_UI_TRUAPI_SHADOW) shadows the local workspace link. Run top-level: make dotli-link"; exit 1)
	@test ! -e "$(DOTLI_UI_HOST_WASM_SHADOW)/package.json" || (echo "$(DOTLI_UI_HOST_WASM_SHADOW) shadows the local workspace link. Run top-level: make dotli-link"; exit 1)
	@node -e 'const fs = require("node:fs"); const checks = [["$(DOTLI_TRUAPI_LINK)/package.json", "@parity/truapi"], ["$(DOTLI_HOST_WASM_LINK)/package.json", "@parity/truapi-host"]]; for (const [path, name] of checks) { const pkg = JSON.parse(fs.readFileSync(path, "utf8")); if (pkg.name !== name) { console.error(path + " resolves " + pkg.name + ", expected local " + name + ". Run: make dotli-link"); process.exit(1); } }'
	cd $(DOTLI_UI) && bun -e 'await import("@parity/truapi-host"); await import("@parity/truapi-host/web");'

dev-cli: cli-runner ## Start the playground (:3000) against the local signing-host CLI; open http://localhost:3000
	cargo build --release -p truapi-host-cli
	cd $(PLAYGROUND) && "$(abspath target/release/truapi-host)" dev -- yarn dev

dev: dev-bootstrap ## Start dotli host (:5173) + playground (:3000) together; open http://localhost:5173/localhost:3000. DEBUG=1 logs wire frames.
	@trap 'kill 0' EXIT; \
	( cd $(DOTLI) && bun run $(DOTLI_PREVIEW) ) & \
	( cd $(PLAYGROUND) && yarn dev ) & \
	( until curl -fsS http://localhost:3000/ >/dev/null 2>&1; do sleep 1; done; curl -fsS http://localhost:3000/diagnostics >/dev/null 2>&1 || true ) & \
	wait

debugger: dev-bootstrap ## Wire debugger (:9231) + a DEV-MODE dotli host (:5173) + playground (:3000). Open http://127.0.0.1:9231
	# `make dev` cannot drive this: dotli ships only production builds, and the dial
	# sits behind `import.meta.env.DEV`, so the host never dials and the board stays
	# empty with no error. Hence a dev-mode host build here.
	#
	# Build every app EXCEPT the host, then the host below. Excluding it by name
	# would put a list of dotli's apps in this repo that a new app there would
	# silently fall off; skipping the siblings entirely leaves apps/sandbox unbuilt
	# and the preview server exits 1, which a stale dist from an earlier `make dev`
	# hides. The host is separate because dotli's own build runs `tsc` and does not
	# typecheck against this repo's newer `@parity/truapi`; `vite build` does not.
	cd $(DOTLI) && VITE_APP_DEBUG=true bunx turbo run build --filter='!@dotli/host'
	cd $(DOTLI)/apps/host && NODE_ENV=development VITE_APP_DEBUG=true \
		VITE_TRUAPI_DEBUGGER_URL=ws://127.0.0.1:$(DEBUGGER_PORT) bunx --bun vite build
	@printf '\n  Debugger:  http://127.0.0.1:$(DEBUGGER_PORT)\n'
	@printf '  Host:      http://localhost:5173/localhost:3000\n'
	@printf '  Another port:  DEBUGGER_PORT=9300 make debugger\n\n'
	# The three servers log for a while after they start, which buries anything
	# printed here. Waiting for the slowest to answer and then reprinting the two
	# URLs puts them at the bottom, where they are still on screen.
	@trap 'kill 0' EXIT; \
	( cd $(DEBUGGER_PKG) && TRUAPI_DEBUGGER_PORT=$(DEBUGGER_PORT) bun run src/server.ts ) & \
	( cd $(DOTLI) && bun scripts/preview-server.ts ) & \
	( cd $(PLAYGROUND) && yarn dev ) & \
	( until curl -sfo /dev/null http://localhost:3000; do sleep 1; done; \
	  printf '\n  Ready.  Debugger:  http://127.0.0.1:$(DEBUGGER_PORT)\n'; \
	  printf '          Host:      http://localhost:5173/localhost:3000\n\n' ) & \
	wait

e2e-dotli: ## Fully automated dotli + playground diagnosis e2e using the local signing-host CLI.
	@$(MAKE) dev-bootstrap
	cargo build -p truapi-host-cli
	cd $(PLAYGROUND) && bun tests/e2e/dotli-diagnosis.ts

e2e-cli-diagnosis: cli-runner ## Full playground diagnosis in a plain browser tab, hosted by `truapi-host dev`.
	cargo build --release -p truapi-host-cli
	cd $(PLAYGROUND) && TRUAPI_HOST_BIN="$(abspath target/release/truapi-host)" bun tests/e2e/cli-diagnosis.ts

e2e-signing-cli: ## Run the generated battery against the direct signing-host CLI.
	scripts/battery.sh --signing-host

e2e-pairing-cli: ## Run the generated battery against the paired pairing-host CLI.
	scripts/battery.sh --pairing-host

e2e-chat-cli: ## Run the Chat content-screening battery against a chat signing-host CLI.
	scripts/battery.sh --chat-host

e2e-pocket-cli: ## Run the Pocket protocol battery against a Pocket signing-host CLI.
	scripts/battery.sh --pocket-host

e2e-cross-product-storage: ## One product reads another's storage on the signing-host CLI, granted by a local product config.
	scripts/cross-product-storage-e2e.sh

e2e-cross-product-ringvrf: ## One product signs with another's ring-VRF key on the signing-host CLI, granted by a local product config.
	scripts/cross-product-ringvrf-e2e.sh

e2e-cross-product-signing: ## One product signs with another's account on the signing-host CLI, granted by a local product config.
	scripts/cross-product-signing-e2e.sh

e2e-cli-update: cli-dist ## Install the packaged truapi-host from a fake release and self-update it, with no network.
	node scripts/e2e-cli-update.mjs

matrix: ## Regenerate the host compatibility matrix from explorer/diagnosis-reports.
	cd $(EXPLORER) && npm run generate-matrix

explorer: ## Run the explorer dev server standalone at http://localhost:5181.
	cd $(EXPLORER) && npx vite --base / --port 5181

IOS_DEVICE_TARGET := aarch64-apple-ios
IOS_SIM_TARGET := aarch64-apple-ios-sim
# Must match the TrUAPIHost Package.swift platforms entry. Without it rustc/cc
# stamp objects with the SDK version and every consumer link emits
# "built for newer iOS version than being linked" warnings.
IOS_DEPLOYMENT_TARGET := 17.0
XCFRAMEWORK_OUT := target/truapi_server.xcframework
XCFRAMEWORK_HEADERS := target/xcframework-headers
# Slices and cargo profile the xcframework is assembled from. The defaults are
# what a release needs; a compile-only consumer overrides both for speed. The
# profile name doubles as cargo's output directory.
# SIM_ONLY=1 drops the device slice while iterating, which halves the target
# builds. publish.sh refuses a framework missing either slice, so this cannot
# reach a release asset. XCFRAMEWORK_TARGETS still overrides both.
#
# 0, false, no and off mean off. Make treats any non-empty value as true, so
# without this SIM_ONLY=0 would drop the device slice.
SIM_ONLY_ON := $(filter-out 0 false no off,$(SIM_ONLY))
XCFRAMEWORK_TARGETS ?= $(if $(SIM_ONLY_ON),$(IOS_SIM_TARGET),$(IOS_DEVICE_TARGET) $(IOS_SIM_TARGET))
XCFRAMEWORK_PROFILE ?= release
XCFRAMEWORK_CARGO_FLAGS := $(if $(filter release,$(XCFRAMEWORK_PROFILE)),--release,)

# One cargo invocation carrying every slice, so the target graphs are scheduled
# together: the release profile's single codegen unit and fat LTO leave a long
# serial tail per slice, which the other slice fills.
xcframework: uniffi ## Build truapi_server.xcframework for iOS device + simulator (SIM_ONLY=1 for simulator only).
	rustup target add $(XCFRAMEWORK_TARGETS)
	IPHONEOS_DEPLOYMENT_TARGET=$(IOS_DEPLOYMENT_TARGET) $(CARGO) build -p truapi \
		$(XCFRAMEWORK_CARGO_FLAGS) \
		$(XCFRAMEWORK_TARGETS:%=--target %)
	rm -rf $(XCFRAMEWORK_OUT) $(XCFRAMEWORK_HEADERS)
	mkdir -p $(XCFRAMEWORK_HEADERS)
	cp $(UNIFFI_SWIFT_TMP)/truapiFFI.h $(XCFRAMEWORK_HEADERS)/
	cp $(UNIFFI_SWIFT_TMP)/truapiFFI.modulemap $(XCFRAMEWORK_HEADERS)/module.modulemap
	slices=""; \
	for target in $(XCFRAMEWORK_TARGETS); do \
		slices="$$slices -library target/$$target/$(XCFRAMEWORK_PROFILE)/libtruapi.a \
			-headers $(XCFRAMEWORK_HEADERS)"; \
	done; \
	xcodebuild -create-xcframework $$slices -output $(XCFRAMEWORK_OUT)
