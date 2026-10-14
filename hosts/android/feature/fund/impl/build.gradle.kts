plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_fund_impl"
}

dependencies {
    api(project(":feature:fund:api"))

    implementation(project(":design"))

    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)

    implementation(project(":feature:swap:api"))
    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:balances:api"))
    implementation(project(":feature:wallet:api"))
    implementation(project(":feature:coinage:api"))
}
