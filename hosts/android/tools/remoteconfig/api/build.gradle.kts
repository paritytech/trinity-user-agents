plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_remoteconfig_api"
}

dependencies {
    // Exposes kotlinx-coroutines (Flow) used in the public service interface.
    api(project(":common"))
}