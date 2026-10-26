import org.jetbrains.kotlin.gradle.tasks.KotlinCompile
import java.util.Properties

// TrUAPI Android host adapter binding.
//
// Compiles the TrUAPI Rust core (`truapi`, from the sibling truapi
// checkout) to `libtruapi.so` per Android ABI via the shared
// `polkadotapp.android.rust` convention (mozilla rust-android-gradle), then
// carries the UniFFI-generated Kotlin bindings + the `io.parity.truapi` host
// adapter shell.
//
// The core crate is read from `truapi.dir`
// in root `local.properties` (or the `TRUAPI_DIR` env var). Point it at any
// local truapi worktree to iterate on the core.
//
// The module is NOT optional: `:feature:products:impl` depends on it in every
// build variant, so the core (and JNA) ship in release APKs too, even though
// the runtime toggle keeps release on the native host. A build without a
// truapi checkout fails at configuration. CI provides the checkout by cloning
// paritytech/trinity-user-agents at the `truapi_ref` pin
// (.github/actions/install/action.yaml) before any compile task runs.

plugins {
    id("polkadotapp.android.rust")
}

// Resolve the truapi checkout that holds `rust/crates/truapi`, using the
// same `truapi.dir` / `TRUAPI_DIR` resolution as the settings-gradle include
// guard, which has already run and accepted whatever this finds: a configured
// checkout if there is one, otherwise the core in the enclosing repository.
//
// Declared above `android { }` because the source sets below interpolate it. A
// `.kts` top-level val read before its initializer yields null, and Gradle
// silently drops a source directory that does not exist, so using it earlier
// produces missing sources rather than an error.
val truapiDir: String = run {
    val localProps = Properties().apply {
        val f = rootProject.file("local.properties")
        if (f.exists()) f.inputStream().use { load(it) }
    }
    val configured = (localProps.getProperty("truapi.dir") ?: System.getenv("TRUAPI_DIR"))
        ?.takeIf { it.isNotBlank() }
    if (configured != null) {
        file(configured).takeIf { it.isAbsolute }?.path
            ?: rootProject.file(configured).path
    } else {
        // Vendored into the core's own repository, where it is two levels up.
        // Same rule as the settings-gradle guard, which has already accepted it.
        //
        // Bare File, not java.io.File: the Android plugin registers a `java`
        // extension on the project, and its generated accessor shadows the root
        // package, so the qualified name does not compile in a build script.
        rootProject.file("../..").takeIf {
            File(it, "rust/crates/truapi").isDirectory
        }?.path
            ?: error("truapi.dir / TRUAPI_DIR must be set to build :bindings:truapi-host")
    }
}

// Generated source root that `syncHostShell` populates with truapi's canonical
// `io.parity.truapi` shell.
val hostShellDir: Provider<Directory> = layout.buildDirectory.dir("truapi-shell")

android {
    namespace = "io.parity.truapi"

    sourceSets {
        getByName("main") {
            java.srcDirs(
                "src/main/kotlin",
                "src/main/kotlin/generated",
                hostShellDir.get().asFile.path,
            )
        }
        getByName("androidTest") { java.srcDirs("src/androidTest/kotlin") }
    }

    lint {
        // Suppresses the NewApi false positive on the UniFFI-generated cleaner
        // (runtime-guarded via Class.forName). See lint.xml.
        lintConfig = file("lint.xml")
    }

    packaging.resources.excludes += "META-INF/versions/9/OSGI-INF/MANIFEST.MF"
}

// truapi owns the `io.parity.truapi` host shell. Sync it from the checkout at
// build time so this repo holds no copy of it and the two cannot drift.
//
// Only the package directory is taken. truapi's `src/main/kotlin` root also holds
// its own generated UniFFI bindings, which would collide with the ones generated
// here (`Redeclaration` / `Conflicting overloads`).
val syncHostShell by tasks.registering(Sync::class) {
    from("$truapiDir/android/truapi-host/src/main/kotlin/io/parity/truapi")
    into(hostShellDir.map { it.dir("io/parity/truapi") })
}

// Every Kotlin compilation here consumes the synced shell and the generated
// bindings, across all four build types and both test source sets.
tasks.withType<KotlinCompile>().configureEach {
    dependsOn(syncHostShell, generateUniffiKotlin)
}

// Override the convention default (`module = "rust/"`) to point at the
// external truapi crate.
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

// Generate the core's wire dispatcher. trinity-user-agents stopped tracking
// `truapi/src/generated` and generates it on demand, so a checkout of any
// commit after that does not compile until this runs. Release tags do not carry
// it either: the iOS tag script commits only the paths `Package.swift` declares.
// Calling the core's own script rather than repeating its codegen invocation
// keeps this from drifting when that pipeline changes.
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

    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.webkit)
    androidTestImplementation(libs.squareup.okhttp3.mockwebserver)
}
