plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_device_sync_api"
}

dependencies {
    implementation(project(":common"))
    implementation(project(":feature:sso:api"))
}
