plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.tools_media_connection_impl"
}

dependencies {

    api(project(":bindings:truapi-host"))
    implementation(project(":tools:media-connection:api"))
    implementation(project(":tools:jwt-auth:api"))
    implementation(project(":common"))

    implementation(libs.bundles.webrtc)

    testImplementation(libs.junit)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.squareup.okhttp3.core)
}
