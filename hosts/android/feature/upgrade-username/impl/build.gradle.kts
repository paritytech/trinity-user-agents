plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_upgrade_username_impl"
}

dependencies {
    api(project(":feature:upgrade-username:api"))

    implementation(libs.androidx.fragment.ktx)

    implementation(project(":feature:people:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:chain-resources:api"))
    implementation(project(":feature:dotns-gateway:api"))

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockk)
    testImplementation(project(":test-shared"))
}
