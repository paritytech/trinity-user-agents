package io.paritytech.polkadotapp.common.di.modules

import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker

// Define an entry point to have access from Composable functions
@EntryPoint
@InstallIn(SingletonComponent::class)
interface PermissionAskerEntryPoint {
    fun permissionAsker(): PermissionAsker
}
