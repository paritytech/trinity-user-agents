plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_chats_transport_protocol"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
}
