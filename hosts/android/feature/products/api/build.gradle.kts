plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_products_api"
}

dependencies {

    implementation(libs.androidx.compose.runtime)

    api(project(":feature:account:api"))
    api(project(":feature:chats:api"))
    api(project(":feature:statement-store:api"))
    api(project(":feature:transactions:api"))
    api(project(":common"))
    api(project(":design"))
    api(project(":bindings:bandersnatch-crypto"))
    api(project(":feature:dotns:api"))
    api(project(":feature:members:api"))
    api(project(":chains"))
    api(project(":tools:ipfs:api"))

    implementation(libs.nova.substrate.serialization)

    testImplementation(libs.junit)
}
