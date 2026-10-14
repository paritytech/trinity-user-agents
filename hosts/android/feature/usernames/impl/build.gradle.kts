plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_usernames_impl"
}

dependencies {
    implementation(libs.androidx.fragment.ktx)

    implementation(project(":chains"))

    implementation(project(":feature:backup:api"))
    implementation(project(":feature:usernames:api"))
    implementation(project(":feature:chain-resources:api"))
    implementation(project(":feature:dotns-gateway:api"))

    implementation(project(":tools:integrity:api"))
    implementation(project(":tools:jwt-auth:api"))

    testImplementation(project(":test-shared"))
    testImplementation(libs.junit)
    testImplementation(libs.mockito.core)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.squareup.retrofit2.core)
}
