plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_scan_impl"
}

dependencies {
    api(project(":feature:scan:api"))

    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)

    implementation(project(":design"))
}
