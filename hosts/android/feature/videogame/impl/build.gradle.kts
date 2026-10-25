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
    implementation(libs.sentry)
    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)

    implementation(project(":common"))
    implementation(project(":design"))
    implementation(project(":database"))
    implementation(project(":bindings:sr25519-vrf"))
    implementation(project(":feature:members:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:xcm:api"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:statement-store:api"))
    implementation(project(":feature:chats:api"))
    implementation(project(":tools:media-connection:api"))
    implementation(project(":feature:transfers:api"))
    implementation(project(":tools:push-notifications:api"))
    implementation(project(":feature:chain-resources:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:products:api"))
    implementation(project(":tools:remoteconfig:api"))
    implementation(project(":feature:usernames:api"))

    implementation(libs.bundles.nova.substrate)

    implementation(libs.androidx.work.runtime)

    implementation(libs.kotlinx.serialization.json)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockito.core)
    testImplementation(libs.mockk)
    testImplementation(project(":test-shared"))
}
