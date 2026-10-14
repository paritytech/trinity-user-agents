plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_transfers_impl"
}

dependencies {
    api(project(":feature:transfers:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":database"))

    implementation(libs.nova.substrate.sdk)

    implementation(project(":feature:account:api"))
    implementation(project(":feature:usernames:api"))
}