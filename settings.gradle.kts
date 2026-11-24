// Gradle workspace root for the Android host packages.
//
// Library modules live one level down at `android/<package>/`. Run
// `gradle :<package>:publish*` from this directory; consumers integrate
// via Maven coordinates, not by including a module from their own
// settings.gradle.kts.
//
// The rest of the repo (Rust crates, JS packages, iOS Swift Package) does
// not use Gradle.

pluginManagement {
    repositories {
        google()
        // Google's mirror of Maven Central. Central rate limits by IP, which CI
        // runners share.
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        // Last, since it redirects what it does not host to Maven Central.
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        // Google's mirror of Maven Central. Central rate limits by IP, which CI
        // runners share.
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
    }
}

rootProject.name = "truapi"

include(":truapi-host")
project(":truapi-host").projectDir = file("android/truapi-host")

include(":truapi-provider")
project(":truapi-provider").projectDir = file("android/truapi-provider")
