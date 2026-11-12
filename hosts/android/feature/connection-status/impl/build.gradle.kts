plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_connection_status_impl"
}

dependencies {
    api(project(":feature:connection-status:api"))
    implementation(project(":feature:statement-store:api"))

    testImplementation(project(":test-shared"))
}
