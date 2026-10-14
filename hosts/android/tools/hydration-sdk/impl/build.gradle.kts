plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.tools_hydration_sdk_impl"
}

dependencies {
    api(project(":tools:hydration-sdk:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":feature:balances:api"))
    implementation(project(":feature:xcm:api"))

    implementation(project(":bindings:hydra-dx-math"))
}