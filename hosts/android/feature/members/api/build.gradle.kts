plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_members_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":bindings:bandersnatch-crypto"))
    api(project(":feature:account:api"))

    testImplementation(libs.junit)
}
