package io.paritytech.polkadotapp.buildlogic

import com.android.build.gradle.BaseExtension
import com.nishtahir.CargoBuildTask
import com.nishtahir.CargoExtension
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.configure
import org.gradle.kotlin.dsl.getByType
import org.gradle.kotlin.dsl.withType

class AndroidRustConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) = with(target) {
        with(pluginManager) {
            apply("polkadotapp.android.library")
            apply("org.mozilla.rust-android-gradle.rust-android")
        }

        extensions.getByType<BaseExtension>().ndkVersion = "29.0.14206865"

        extensions.configure<CargoExtension> {
            module = "rust/"
            targets = listOf("arm", "arm64", "x86", "x86_64")
            profile = "release"
        }

        tasks.matching { it.name.matches(Regex("merge.*JniLibFolders")) }.configureEach {
            inputs.dir(layout.buildDirectory.dir("rustJniLibs/android"))
            dependsOn("cargoBuild")
        }

        tasks.withType(CargoBuildTask::class.java).configureEach {
            notCompatibleWithConfigurationCache("rust-android-gradle CargoBuildTask invokes Task.project at execution time")
        }
    }
}
