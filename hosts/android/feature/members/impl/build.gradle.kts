plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_members_impl"
}

dependencies {
    api(project(":feature:members:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)
}
