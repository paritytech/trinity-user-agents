plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_videogame_impl"
}

dependencies {
    implementation(libs.hilt.lifecycle.viewmodel.compose)

    implementation(project(":common"))
    implementation(project(":design"))
    implementation(project(":feature:chats:api"))
    implementation(project(":feature:products:api"))

    implementation(libs.kotlinx.serialization.json)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockito.core)
    testImplementation(libs.mockk)
    testImplementation(project(":test-shared"))
}
