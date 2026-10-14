plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_pgas_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":bindings:bandersnatch-crypto"))
    api(project(":feature:transactions:api"))
    api(project(":feature:people:api"))
    api(project(":feature:tokens:api"))
}
