plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.parcelize)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_chats_api"
}

dependencies {

    implementation(libs.androidx.compose.runtime)
    implementation(libs.hilt.lifecycle.viewmodel.compose)

    api(project(":feature:tokens:api"))
    api(project(":feature:account:api"))
    api(project(":feature:usernames:api"))
    api(project(":feature:coinage:api"))
    api(project(":feature:statement-store:api"))
    api(project(":tools:push-notifications:api"))

    api(project(":common"))
    api(project(":design"))
    api(project(":chains"))
}
