plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_balances_impl"
}

dependencies {
    api(project(":feature:balances:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(libs.nova.substrate.sdk)

    implementation(project(":database"))
    implementation(project(":feature:account:api"))

    testImplementation(project(":test-shared"))
}