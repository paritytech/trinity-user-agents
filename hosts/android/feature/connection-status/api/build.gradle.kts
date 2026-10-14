plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_connection_status_api"
}

dependencies {
    api(project(":common"))
    api(project(":design"))
    api(project(":chains"))

    testImplementation(project(":test-shared"))

    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.compose.ui.test.manifest)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
