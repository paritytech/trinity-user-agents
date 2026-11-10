// TrUAPI Android chain transport.
//
// Publishes `io.parity:truapi-provider-android` to Maven: the UniFFI Kotlin
// bindings for the `truapi-provider` crate (embedded smoldot light client plus
// the bundled chain-spec catalog), together with `libtruapi_provider.so` per
// Android ABI. Hosts address a chain by genesis hash and exchange raw JSON-RPC
// strings; the crate owns the light client and the specs.
//
// Bundling the cdylib is what distinguishes this from `truapi-host`, whose AAR
// leaves it to the integrator. It makes the jniLibs a publish-time requirement
// rather than an optional extra, enforced by `verifyJniLibs` below.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

android {
    namespace = "io.parity.truapi.provider"
    compileSdk = 34

    lint {
        // Suppresses the NewApi false positive on the UniFFI-generated cleaner
        // (runtime-guarded via Class.forName). See lint.xml.
        lintConfig = file("lint.xml")
    }

    defaultConfig {
        // Matches the truapi-host floor so a host can depend on both.
        minSdk = 29
        consumerProguardFiles("consumer-rules.pro")
    }

    sourceSets {
        getByName("main") {
            java.srcDirs("src/main/kotlin")
            manifest.srcFile("src/main/AndroidManifest.xml")
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
            withJavadocJar()
        }
    }
}

dependencies {
    // UniFFI Kotlin bindings use JNA for FFI.
    api("net.java.dev.jna:jna:5.14.0@aar")
    // UniFFI async functions and callbacks use cancellable continuations and jobs.
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
}

// Coordinates for the local Maven publication (`publishToMavenLocal`), which is
// the only publication that exists today. JitPack cannot serve this module: it
// builds from a git tag, and both the bindings and the cdylib are generated
// rather than committed. A remote coordinate needs a hosted Maven repository.
val publicationGroup = "io.parity"
val publicationArtifact = "truapi-provider-android"
val publicationVersion = "0.1.0"

group = publicationGroup
version = publicationVersion

// A publication without the cdylib would resolve and then fail at the first
// `ChainProvider()` with UnsatisfiedLinkError, so refuse to publish one.
val verifyJniLibs =
    tasks.register("verifyJniLibs") {
        description = "Fails when src/main/jniLibs has no .so, which would publish an unusable AAR."
        doLast {
            val libs = file("src/main/jniLibs")
            val found = libs.walkTopDown().filter { it.name == "libtruapi_provider.so" }.toList()
            if (found.isEmpty()) {
                throw GradleException(
                    "no libtruapi_provider.so under ${libs.path} — run `make android-jni-provider` first"
                )
            }
            logger.lifecycle("bundling ${found.size} ABI(s): ${found.map { it.parentFile.name }}")
        }
    }

tasks.matching { it.name.startsWith("publish") }.configureEach { dependsOn(verifyJniLibs) }

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = publicationGroup
            artifactId = publicationArtifact
            version = publicationVersion

            afterEvaluate {
                from(components["release"])
            }

            pom {
                name.set("TrUAPI Android chain transport")
                description.set(
                    "Kotlin bindings for the TrUAPI provider crate (UniFFI): an " +
                        "embedded smoldot light client with a bundled chain-spec " +
                        "catalog, addressed by genesis hash. Bundles " +
                        "libtruapi_provider.so, so no Rust toolchain is required."
                )
                url.set("https://github.com/paritytech/trinity-user-agents")
                licenses {
                    license {
                        name.set("MIT")
                        url.set("https://github.com/paritytech/trinity-user-agents/blob/main/LICENSE")
                    }
                    license {
                        name.set("Apache-2.0")
                        url.set("https://github.com/paritytech/trinity-user-agents/blob/main/LICENSE-APACHE")
                    }
                }
                scm {
                    connection.set("scm:git:https://github.com/paritytech/trinity-user-agents.git")
                    developerConnection.set("scm:git:ssh://git@github.com/paritytech/trinity-user-agents.git")
                    url.set("https://github.com/paritytech/trinity-user-agents")
                }
                developers {
                    developer {
                        name.set("Parity Technologies")
                        email.set("admin@parity.io")
                        organization.set("Parity Technologies")
                        organizationUrl.set("https://parity.io")
                    }
                }
            }
        }
    }

    repositories {
        // `gradle publishToMavenLocal` during development; consumers resolve it
        // through `mavenLocal()` until a hosted repository is wired up.
        mavenLocal()
    }
}
