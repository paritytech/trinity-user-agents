plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_coinage_impl"
}

dependencies {
    api(project(":feature:coinage:api"))
    api(project(":feature:transactions:api"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:balances:api"))
    implementation(project(":feature:products:api"))
    implementation(project(":feature:revive:api"))
    implementation(project(":feature:swap:api"))
    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:usernames:api"))

    implementation(project(":tools:remoteconfig:api"))

    implementation(project(":database"))

    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    testImplementation(libs.kotlinx.coroutines.test)
}
