import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

// TrUAPI Android host adapter binding.
//
// Compiles the TrUAPI Rust core (`truapi`, from the enclosing
// trinity-user-agents tree) to `libtruapi.so` per Android ABI via the shared
// `polkadotapp.android.rust` convention (mozilla rust-android-gradle), then
// carries the UniFFI-generated Kotlin bindings + the `io.parity.truapi` host
// adapter shell.
//
// The module is NOT optional: `:feature:products:impl` depends on it in every
// build variant, so the core (and JNA) ship in release APKs too, even though
// the runtime toggle keeps release on the native host.

plugins {
    id("polkadotapp.android.rust")
}

// The core, two levels up in the repository this tree is vendored into.
//
// Declared above `android { }` because the source sets below interpolate it. A
// `.kts` top-level val read before its initializer yields null, and Gradle
// silently drops a source directory that does not exist, so using it earlier
// produces missing sources rather than an error.
val truapiDir: String = rootProject.file("../..").path

// Handwritten Kotlin adapters and value types synced from the core checkout.
val hostShellDir: Provider<Directory> = layout.buildDirectory.dir("truapi-shell")

// The signed stacks the app can run, synced from the core checkout as `stacks/` assets.
val stackAssetsDir: Provider<Directory> = layout.buildDirectory.dir("truapi-stacks")

android {
    namespace = "io.parity.truapi"

    sourceSets {
        getByName("main") {
            java.srcDirs(
                "src/main/kotlin",
                "src/main/kotlin/generated",
                hostShellDir.get().asFile.path,
            )
            assets.srcDir(stackAssetsDir.get().asFile.path)
        }
        getByName("test") { java.srcDirs("src/test/kotlin") }
        getByName("androidTest") { java.srcDirs("src/androidTest/kotlin") }
    }

    lint {
        // Suppresses the NewApi false positive on the UniFFI-generated cleaner
        // (runtime-guarded via Class.forName). See lint.xml.
        lintConfig = file("lint.xml")
    }

    packaging.resources.excludes += "META-INF/versions/9/OSGI-INF/MANIFEST.MF"
}

// truapi owns the `io.parity.truapi` host shell. Sync it from the core at
// build time so this tree holds no copy of it and the two cannot drift.
//
// Generated bindings are excluded because this module generates its own copy.
val syncHostShell by tasks.registering(Sync::class) {
    from("$truapiDir/android/truapi-host/src/main/kotlin") { exclude("generated/**") }
    into(hostShellDir)
}

// truapi owns the signed stack descriptors too. The app ships the three it can
// run as `stacks/<name>.link` and `stacks/<name>.json` assets, synced from the
// core so this tree holds no copy of them either.
val syncStacks by tasks.registering(Sync::class) {
    from("$truapiDir/stacks") { include("previewnet.*", "paseo-next-v2.*", "polkadot.*") }
    into(stackAssetsDir.map { it.dir("stacks") })
}

// Asset merging and lint both read the synced stacks.
tasks.matching { (it.name.startsWith("merge") && it.name.endsWith("Assets")) || "lint" in it.name.lowercase() }
    .configureEach { dependsOn(syncStacks) }

// Every Kotlin compilation here consumes the synced shell and the generated
// bindings, across all four build types and both test source sets.
tasks.withType<KotlinCompile>().configureEach {
    dependsOn(syncHostShell, generateUniffiKotlin)
}

// Override the convention default (`module = "rust/"`) to point at the core's
// truapi crate.
cargo {
    module = "$truapiDir/rust/crates/truapi"
    libname = "truapi"
    // Narrows the convention's four ABIs to the three truapi itself builds.
    // The core is ~9MB per ABI, by far the largest binding here, and 32-bit x86
    // only ever serves a long-obsolete emulator image.
    targets = listOf("arm", "arm64", "x86_64")
    // truapi is a workspace member, so cargo emits artifacts to the
    // workspace target dir, not a crate-local one. Point the plugin there so it
    // can find and copy the per-ABI .so into rustJniLibs.
    targetDirectory = "$truapiDir/target"
}

// The nightly rustdoc the core's codegen runs on, pinned by date in
// gradle.properties: its JSON format moves with the toolchain, so an unpinned
// nightly turns every build red the day truapi-codegen stops accepting it.
val rustNightly: String = providers.gradleProperty("truapi.rustNightly").get()

// Host cdylib uniffi-bindgen reads to extract the interface: .dylib on macOS,
// .so on Linux, .dll on Windows. Built with truapi's `codegen` profile, which
// that repo designates for binding generation because `[profile.release]` sets
// `strip = "symbols"` and can drop the metadata symbols uniffi-bindgen scans.
// Using it also means one cargo build serves both this task and truapi's own
// `make uniffi-kotlin`.
val hostCdylib: String = run {
    val os = System.getProperty("os.name").lowercase()
    val ext = when {
        os.contains("mac") || os.contains("darwin") -> "dylib"
        os.contains("win") -> "dll"
        else -> "so"
    }
    "$truapiDir/target/codegen/libtruapi.$ext"
}

// codegen.sh formats what it emits with the core's own prettier, resolved with
// `npm exec --no`, so the core's workspace dependencies have to be installed
// before it runs. trinity-user-agents's own CI does the same thing ahead of the
// script; without it the task dies on a missing prettier rather than anything
// to do with the generated code.
val installCoreNodeDeps by tasks.registering(Exec::class) {
    workingDir = file(truapiDir)
    commandLine("npm", "ci", "--ignore-scripts")
    inputs.files("$truapiDir/package.json", "$truapiDir/package-lock.json")
        .withPropertyName("coreNodeManifests")
    outputs.dir("$truapiDir/node_modules").withPropertyName("coreNodeModules")
}

// Generate the core's wire dispatcher. `truapi/src/generated` is gitignored,
// so the core does not compile until this runs. Calling the core's own script
// rather than repeating its codegen invocation keeps this from drifting when
// that pipeline changes.
val generateCoreDispatcher by tasks.registering(Exec::class) {
    dependsOn(installCoreNodeDeps)
    workingDir = file(truapiDir)
    environment("TRUAPI_NIGHTLY_TOOLCHAIN", rustNightly)
    commandLine("./scripts/codegen.sh")
    inputs.files(
        fileTree("$truapiDir/rust/crates/truapi") { include("**/*.rs", "**/Cargo.toml") },
        fileTree("$truapiDir/rust/crates/truapi-provider") { include("**/*.rs", "**/Cargo.toml") },
        fileTree("$truapiDir/rust/crates/truapi-codegen") { include("**/*.rs", "**/Cargo.toml") },
    ).withPropertyName("codegenSources")
    outputs.dirs(
        "$truapiDir/rust/crates/truapi/src/generated",
        "$truapiDir/rust/crates/truapi/src/wasm",
    ).withPropertyName("generatedDispatcher")
}

// Build the host-native cdylib for uniffi-bindgen — a runner-targeted build,
// separate from the per-ABI Android cross-compile (cargoBuild). Cargo is
// incremental on its own; the input/output declarations additionally let
// Gradle skip the cargo invocation entirely when the rust tree is untouched.
val buildHostCdylib by tasks.registering(Exec::class) {
    dependsOn(generateCoreDispatcher)
    workingDir = file(truapiDir)
    commandLine("cargo", "build", "-p", "truapi", "--profile", "codegen")
    inputs.files(
        fileTree("$truapiDir/rust/crates") { include("**/*.rs", "**/*.js", "**/Cargo.toml") },
        "$truapiDir/Cargo.toml",
        "$truapiDir/Cargo.lock",
    ).withPropertyName("rustSources")
    outputs.file(hostCdylib).withPropertyName("hostCdylib")
}

// Regenerate the UniFFI Kotlin bindings from the host cdylib. Wired into
// compileKotlin, incremental via the cdylib input, and still runnable
// standalone (`./gradlew :bindings:truapi-host:generateUniffiKotlin` or
// truapi's `make uniffi-kotlin`).
val generateUniffiKotlin by tasks.registering(Exec::class) {
    dependsOn(buildHostCdylib)
    val outDir = layout.projectDirectory.dir("src/main/kotlin/generated")
    doFirst { outDir.asFile.mkdirs() }
    workingDir = file(truapiDir)
    commandLine(
        "cargo", "run", "-p", "uniffi-bindgen-cli", "--", "generate",
        "--library", hostCdylib,
        "--language", "kotlin",
        "--no-format",
        "--out-dir", outDir.asFile.absolutePath,
    )
    inputs.file(hostCdylib).withPropertyName("hostCdylib")
    inputs.file("$truapiDir/rust/crates/truapi/uniffi.toml").withPropertyName("uniffiConfig")
    outputs.dir(outDir).withPropertyName("generatedBindings")
}

// The per-ABI cross-compiles build truapi too, so they need the
// dispatcher just as much as the host-native build does.
tasks.matching { it.name.startsWith("cargoBuild") }
    .configureEach { dependsOn(generateCoreDispatcher) }

extra["truapiSourceDir"] = file(truapiDir)
apply(from = file("$truapiDir/android/truapi-container.gradle"))

dependencies {
    // UniFFI Kotlin bindings use JNA for FFI.
    api("net.java.dev.jna:jna:5.14.0@aar")

    // The generated bindings bridge the core's async callbacks onto coroutines.
    api(libs.kotlinx.coroutines.core)

    // OkHttp WebSocket backs the chain JSON-RPC transport (WebSocketChainProvider).
    implementation(libs.squareup.okhttp3.core)

    testImplementation(libs.junit)
    testImplementation(libs.squareup.okhttp3.mockwebserver)

    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.webkit)
    androidTestImplementation(libs.squareup.okhttp3.mockwebserver)
}
