plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_coinage_api"
}

dependencies {
    api(project(":chains"))
    api(project(":common"))
    api(project(":feature:people:api"))
    api(project(":feature:members:api"))
    api(project(":feature:transactions:api"))

    api(project(":bindings:bandersnatch-crypto"))

    implementation(project(":feature:tokens:api"))
}
