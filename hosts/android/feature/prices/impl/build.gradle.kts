plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_prices_impl"
}

dependencies {
    api(project(":feature:prices:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(libs.nova.substrate.sdk)

    implementation(project(":database"))
}
