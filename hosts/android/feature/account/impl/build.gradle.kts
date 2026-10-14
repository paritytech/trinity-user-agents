plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_account_impl"
}

dependencies {
    api(project(":feature:account:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":database"))

    testImplementation(project(":test-shared"))
}