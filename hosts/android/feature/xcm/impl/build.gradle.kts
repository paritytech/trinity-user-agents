plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_xcm_impl"
}

dependencies {
    api(project(":feature:xcm:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(libs.nova.substrate.sdk)

    implementation(project(":tools:remoteconfig:api"))

    testImplementation(project(":test-shared"))
}