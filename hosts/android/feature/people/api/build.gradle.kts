plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_people_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":design"))
    api(project(":bindings:bandersnatch-crypto"))
    api(project(":feature:account:api"))
    api(project(":feature:transactions:api"))
    api(project(":feature:members:api"))
    api(project(":feature:dotns:api"))

    implementation(libs.hilt.android)

    testImplementation(project(":test-shared"))
}
