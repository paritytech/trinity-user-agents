plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.test_shared"
}

dependencies {
    api(project(":common"))

    api(libs.mockito.core)
    api(libs.junit)
    api(libs.kotlinx.coroutines.test)

    api(libs.google.gson)
    api(libs.nova.substrate.sdk)
}