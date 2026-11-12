plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_statement_store_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":feature:account:api"))
    api(project(":feature:people:api"))

    api(libs.bouncycastle.jdk18)
}
