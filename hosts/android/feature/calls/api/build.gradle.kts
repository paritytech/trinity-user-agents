plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_calls_api"
}

dependencies {
    implementation(project(":feature:chats:api"))
    implementation(project(":tools:media-connection:api"))
}
