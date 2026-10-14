plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_transaction_storage_impl"
}

dependencies {
    api(project(":feature:transaction-storage:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":chains"))
    implementation(project(":common"))
    implementation(project(":bindings:bandersnatch-crypto"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:members:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:chain-resources:api"))
}
