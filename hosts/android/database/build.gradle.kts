plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.androidx.room)
}

android {
    namespace = "io.paritytech.polkadotapp.database"
}

room {
    schemaDirectory("$projectDir/schemas")
}

dependencies {
    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":common"))

    api(libs.androidx.room.runtime)
    implementation(libs.androidx.room.ktx)
    ksp(libs.androidx.room.compiler)

    implementation(libs.google.gson)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.room.testing)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(project(":feature:chats:impl"))
}
