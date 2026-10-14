plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_jwt_auth_impl"
}

dependencies {
    api(project(":tools:jwt-auth:api"))
    implementation(project(":tools:integrity:api"))

    testImplementation(project(":test-shared"))
    testImplementation(libs.junit)
    testImplementation(libs.mockito.core)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.squareup.retrofit2.core)
}
