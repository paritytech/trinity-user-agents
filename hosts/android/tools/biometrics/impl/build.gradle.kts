import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_biometrics_impl"
}

dependencies {
    api(project(":tools:biometrics:api"))

    implementation(libs.androidx.biometric)
}