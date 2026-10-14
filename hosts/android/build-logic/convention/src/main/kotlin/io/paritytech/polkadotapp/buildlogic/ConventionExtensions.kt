package io.paritytech.polkadotapp.buildlogic

import com.android.build.gradle.BaseExtension
import dev.detekt.gradle.Detekt
import dev.detekt.gradle.extensions.DetektExtension
import org.gradle.api.JavaVersion
import org.gradle.api.Project
import org.gradle.api.artifacts.VersionCatalog
import org.gradle.api.artifacts.VersionCatalogsExtension
import org.gradle.kotlin.dsl.configure
import org.gradle.kotlin.dsl.dependencies
import org.gradle.kotlin.dsl.getByType
import org.gradle.kotlin.dsl.withType
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

internal val Project.libs: VersionCatalog
    get() = extensions.getByType<VersionCatalogsExtension>().named("libs")

internal fun Project.configureAndroidCommon(extension: BaseExtension) {
    // substrate-sdk 2.12.0 moved to bcprov-jdk18on; the legacy coordinate ships the same classes
    // under different artifact ids, which fails dex merging. web3j still pulls it in transitively.
    configurations.all {
        exclude(mapOf("group" to "org.bouncycastle", "module" to "bcprov-jdk15on"))
    }

    configureAndroidExtension(extension)
}

private fun configureAndroidExtension(extension: BaseExtension) = with(extension) {
    compileSdkVersion(36)

    defaultConfig {
        minSdk = 29
        targetSdk = 36

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildFeatures.buildConfig = true
    buildFeatures.viewBinding = true

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    packagingOptions {
        resources.excludes.add("META-INF/INDEX.LIST")
        resources.excludes.add("META-INF/DEPENDENCIES")
    }

    buildTypes {
        getByName("debug") {
            isShrinkResources = false
            isMinifyEnabled = false
        }
        getByName("release") {
            isShrinkResources = false
            isMinifyEnabled = false

            lintOptions {
                isAbortOnError = false
            }
        }
        create("nightly") {
            initWith(getByName("debug"))
        }
        create("safetynet") {
            initWith(getByName("nightly"))
        }
    }
}

internal fun Project.configureKotlin() {
    tasks.withType<KotlinCompile>().configureEach {
        compilerOptions {
            freeCompilerArgs.add("-Xcontext-parameters")
            freeCompilerArgs.add("-XXLanguage:+ExplicitBackingFields")
            freeCompilerArgs.add("-opt-in=kotlin.time.ExperimentalTime")

            jvmTarget.set(JvmTarget.JVM_21)
        }
    }
}

internal fun Project.configureDetekt() {
    extensions.configure<DetektExtension> {
        config.setFrom(rootProject.file("config/detekt/config.yml"))
        parallel.set(true)
        autoCorrect.set(true)
    }

    tasks.withType<Detekt>().configureEach {
        // Generated sources are not ours to style-check, and autoCorrect would
        // rewrite them. The UniFFI bindings under bindings/truapi-host live in
        // src/, so detekt sees them; other generators emit into build/.
        exclude("**/generated/**")
        reports {
            html.required.set(true)
            sarif.required.set(false)
        }
    }

    dependencies {
        add("detektPlugins", libs.findLibrary("detekt-formatting").get())
        add("detektPlugins", libs.findLibrary("detekt-compose-rules").get())
    }
}
