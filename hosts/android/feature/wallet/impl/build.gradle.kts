plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_wallet_impl"
}

dependencies {
    api(project(":feature:wallet:api"))

    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)

    implementation(libs.nova.substrate.sdk)
    implementation(libs.kotlinx.serialization.json)

    implementation(project(":common"))
    implementation(project(":chains"))
    implementation(project(":design"))

    implementation(project(":feature:chats:api"))
    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:balances:api"))
    implementation(project(":feature:transfers:api"))
    implementation(project(":feature:usernames:api"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:prices:api"))
    implementation(project(":feature:coinage:api"))
    implementation(project(":feature:scan:api"))
    implementation(project(":feature:fund:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:products:api"))
    implementation(project(":tools:remoteconfig:api"))

    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)
}
