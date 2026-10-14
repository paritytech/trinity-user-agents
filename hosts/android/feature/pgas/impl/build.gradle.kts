plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_pgas_impl"
}

dependencies {
    api(project(":feature:pgas:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":common"))
    implementation(project(":chains"))
    implementation(project(":bindings:bandersnatch-crypto"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:members:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:balances:api"))
}
