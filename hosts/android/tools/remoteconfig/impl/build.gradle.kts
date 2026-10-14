plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_remoteconfig_impl"
}

dependencies {
    api(project(":tools:remoteconfig:api"))

    implementation(project(":tools:common"))

    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.remote.config)
}