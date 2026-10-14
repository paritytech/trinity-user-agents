package io.paritytech.polkadotapp.buildlogic

import com.android.build.gradle.BaseExtension
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.getByType

class AndroidLibraryConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) = with(target) {
        with(pluginManager) {
            apply("com.android.library")
            apply("org.jetbrains.kotlin.android")
            apply("com.google.devtools.ksp")
            apply("dev.detekt")
        }

        configureAndroidCommon(extensions.getByType<BaseExtension>())
        configureKotlin()
        configureDetekt()
    }
}
