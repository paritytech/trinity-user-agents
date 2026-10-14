import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    `kotlin-dsl`
}

group = "io.paritytech.polkadotapp.buildlogic"

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks.withType<KotlinCompile>().configureEach {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_21)
    }
}

dependencies {
    compileOnly(libs.android.gradlePlugin)
    compileOnly(libs.kotlin.gradlePlugin)
    compileOnly(libs.compose.gradlePlugin)
    compileOnly(libs.ksp.gradlePlugin)
    compileOnly(libs.detekt.gradlePlugin)
    compileOnly(libs.hilt.gradlePlugin)
    compileOnly(libs.rust.gradlePlugin)
}

gradlePlugin {
    plugins {
        register("androidLibrary") {
            id = "polkadotapp.android.library"
            implementationClass = "io.paritytech.polkadotapp.buildlogic.AndroidLibraryConventionPlugin"
        }
        register("androidApplication") {
            id = "polkadotapp.android.application"
            implementationClass = "io.paritytech.polkadotapp.buildlogic.AndroidApplicationConventionPlugin"
        }
        register("androidCompose") {
            id = "polkadotapp.android.compose"
            implementationClass = "io.paritytech.polkadotapp.buildlogic.AndroidComposeConventionPlugin"
        }
        register("androidHilt") {
            id = "polkadotapp.android.hilt"
            implementationClass = "io.paritytech.polkadotapp.buildlogic.HiltConventionPlugin"
        }
        register("androidRust") {
            id = "polkadotapp.android.rust"
            implementationClass = "io.paritytech.polkadotapp.buildlogic.AndroidRustConventionPlugin"
        }
    }
}
