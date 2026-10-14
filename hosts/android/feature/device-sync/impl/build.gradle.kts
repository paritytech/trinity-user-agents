plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_device_sync_impl"
}

dependencies {

    implementation(libs.nova.substrate.serialization)

    implementation(project(":feature:device-sync:api"))
    implementation(project(":feature:sso:api"))
    implementation(project(":feature:chats:api"))
    implementation(project(":feature:chats:transport-protocol"))
    implementation(project(":feature:statement-store:api"))
    implementation(project(":feature:account:api"))
    implementation(project(":tools:media-connection:api"))
    implementation(project(":chains"))

    implementation(project(":common"))

    testImplementation(project(":test-shared"))
}
