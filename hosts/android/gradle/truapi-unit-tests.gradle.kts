evaluationDependsOn(":bindings:truapi-host")

val truapiBindings = project(":bindings:truapi-host")
val truapiSourceDir = truapiBindings.extra["truapiSourceDir"] as File

// The Android JNA artifact lacks the desktop dispatch library used by JVM tests.
dependencies.add("testRuntimeOnly", "net.java.dev.jna:jna:5.14.0")

tasks.withType<Test>().configureEach {
    dependsOn(truapiBindings.tasks.named("buildHostCdylib"))
    systemProperty("jna.library.path", File(truapiSourceDir, "target/codegen").path)
}
