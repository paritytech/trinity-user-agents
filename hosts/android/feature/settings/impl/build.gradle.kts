plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_settings_impl"
}

dependencies {
    api(project(":feature:settings:api"))

    implementation(project(":chains"))
    implementation(project(":common"))
    implementation(project(":design"))

    implementation(project(":feature:account:api"))
    implementation(project(":feature:chats:api"))
    implementation(project(":feature:backup:api"))
    implementation(project(":feature:coinage:api"))
    implementation(project(":feature:prices:api"))
    implementation(project(":feature:sso:api"))
    implementation(project(":feature:tokens:api"))

    implementation(project(":tools:auth:api"))
    implementation(project(":tools:backup:api"))
    implementation(project(":tools:biometrics:api"))

    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)

    testImplementation(project(":test-shared"))
}
