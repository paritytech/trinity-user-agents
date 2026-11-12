// TrUAPI Android host adapter.
//
// Publishes `io.parity:truapi-host-android` to Maven. Products running in a
// `WebView` connect to the Rust core via its localhost WebSocket bridge
// (`TrUAPIProductExecution.startWsBridge`); the Rust core (compiled to
// `libtruapi.so`) handles wire decoding, routing, subscription
// lifecycle, and host capability dispatch.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

android {
    namespace = "io.parity.truapi"
    compileSdk = 34

    lint {
        // Suppresses the NewApi false positive on the UniFFI-generated cleaner
        // (runtime-guarded via Class.forName). See lint.xml.
        lintConfig = file("lint.xml")
    }

    defaultConfig {
        // minSdk 29 matches the polkadot-app-android-v2 floor; raise here
        // first and bump consumers' floors if we ever depend on a newer API.
        minSdk = 29
        consumerProguardFiles("consumer-rules.pro")
    }

    sourceSets {
        getByName("main") {
            java.srcDirs("src/main/kotlin")
            manifest.srcFile("src/main/AndroidManifest.xml")
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
    // UniFFI async functions and callbacks use cancellable continuations and
    // jobs, and `TrUAPIProductExecution.render` returns a `Flow`, so consumers
    // compile against this.
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
}

extra["truapiSourceDir"] = rootProject.projectDir
apply(from = rootProject.file("android/truapi-container.gradle"))

// Coordinates for the Maven publication. Releases are published to GitHub
// Packages by .github/workflows/release-android.yml, which passes the real
// version via -PtruapiHostVersion; local publishes default to 0.0.0-local.
val publicationGroup = "io.parity"
val publicationArtifact = "truapi-host-android"
val publicationVersion = (findProperty("truapiHostVersion") as String?) ?: "0.0.0-local"

group = publicationGroup
version = publicationVersion

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
                name.set("TrUAPI Android host adapter")
                description.set(
                    "Kotlin wrapper around the TrUAPI Rust core (UniFFI). " +
                        "Hosts integrating a `WebView`-based product link the " +
                        "`libtruapi` cdylib and route product traffic " +
                        "through the localhost WebSocket bridge."
                )
                url.set("https://github.com/paritytech/trinity-user-agents")
                licenses {
                    license {
                        name.set("MIT")
                        url.set("https://github.com/paritytech/trinity-user-agents/blob/main/LICENSE")
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
        // Maven Local for `gradle publishToMavenLocal` during development.
        mavenLocal()
        // Release target: the release-android workflow publishes here with
        // the workflow's GITHUB_TOKEN.
        maven {
            name = "GitHubPackages"
            url = uri("https://maven.pkg.github.com/paritytech/trinity-user-agents")
            credentials {
                username = System.getenv("GITHUB_ACTOR")
                password = System.getenv("GITHUB_TOKEN")
            }
        }
    }
}
