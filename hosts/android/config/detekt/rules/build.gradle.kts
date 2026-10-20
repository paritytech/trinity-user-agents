import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_21)
        // detekt 2.0.0-alpha.5 is built with Kotlin 2.4 metadata, which the project's Kotlin 2.2 compiler refuses to read
        freeCompilerArgs.add("-Xskip-metadata-version-check")
    }
}

dependencies {
    compileOnly(libs.detekt.api)

    // detekt-test 2.0.0-alpha.5 requests a detekt-api test-fixtures capability that has no runtime variant
    testImplementation(libs.detekt.test) {
        exclude(group = "dev.detekt", module = "detekt-api")
    }
    testImplementation(libs.detekt.api)
    testImplementation(libs.junit)
}
