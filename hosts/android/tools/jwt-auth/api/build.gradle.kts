plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_jwt_auth_api"
}

dependencies {
    api(project(":common"))
    // AuthError.Integrity carries an IntegrityError, so consumers need it on their compile path.
    api(project(":tools:integrity:api"))

    implementation(libs.hilt.android)
}
