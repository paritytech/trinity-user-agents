import com.google.protobuf.gradle.id

plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.google.protobuf)
}

android {
    namespace = "io.paritytech.polkadotapp.tools_car_parser"
}

protobuf {
    protoc {
        artifact = libs.protobuf.protoc.get().toString()
    }
    generateProtoTasks {
        all().forEach { task ->
            task.builtins {
                id("java") {
                    option("lite")
                }
            }
        }
    }
}

dependencies {
    api(project(":common"))
    implementation(project(":tools:ipfs:api"))

    implementation(libs.kotlinx.serialization.cbor)
    implementation(libs.protobuf.javalite)

    testImplementation(project(":test-shared"))
}
