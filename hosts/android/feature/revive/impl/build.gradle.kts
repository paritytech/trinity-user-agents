plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_revive_impl"
}

dependencies {
    api(project(":feature:revive:api"))

    implementation(project(":common"))
    implementation(project(":chains"))

    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)
}
