plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_transaction_storage_api"
}

dependencies {
    api(project(":chains"))
    api(project(":bindings:bandersnatch-crypto"))
    api(project(":feature:transactions:api"))
    api(project(":feature:products:api"))
    api(project(":feature:people:api"))
    api(project(":feature:account:api"))
}
