pluginManagement {
    repositories {
        google()
        // Google's mirror of Maven Central. Central rate limits by IP, which CI
        // runners share.
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        // Google's mirror of Maven Central. Central rate limits by IP, which CI
        // runners share.
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        gradlePluginPortal()
    }
    versionCatalogs {
        create("libs") {
            from(files("../gradle/libs.versions.toml"))
        }
    }
}

rootProject.name = "build-logic"
include(":convention")
