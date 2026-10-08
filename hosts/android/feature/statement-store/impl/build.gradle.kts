plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_statement_store_impl"
}

dependencies {
    api(project(":feature:statement-store:api"))

    implementation(libs.hilt.android)
    implementation(libs.hilt.androidx.work)
    ksp(libs.hilt.android.compiler)
    ksp(libs.hilt.androidx.compiler)

    implementation(libs.androidx.work.runtime)

    implementation(project(":database"))
    implementation(project(":chains"))
    implementation(project(":bindings:bandersnatch-crypto"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:members:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:chain-resources:api"))

    testImplementation(project(":test-shared"))
}
