package io.paritytech.polkadotapp.tools_auth_impl.firebase

import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.auth.FirebaseUser
import com.google.firebase.auth.GoogleAuthProvider
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.tools_auth_api.FirebaseAuthManager
import io.paritytech.polkadotapp.tools_auth_api.GoogleAuthManager
import io.paritytech.polkadotapp.tools_common.executeSuspend
import javax.inject.Inject

class RealFirebaseAuthManager @Inject constructor(
    private val googleAuthManager: GoogleAuthManager,
) : FirebaseAuthManager {
    private val auth = FirebaseAuth.getInstance()

    override suspend fun authenticate(scope: String?): Result<String> {
        val currentFbUser = auth.currentUser

        if (currentFbUser != null) {
            return Result.success(currentFbUser.uid)
        }
        return googleAuthManager.runAuthenticated(scope) {
            authFirebaseUserWithGoogle(it)
        }
            .map { it.uid }
    }

    private suspend fun authFirebaseUserWithGoogle(token: String?): Result<FirebaseUser> {
        val credential = GoogleAuthProvider.getCredential(token, null)
        if (auth.currentUser != null) auth.signOut()

        return auth.signInWithCredential(credential)
            .executeSuspend()
            .map { it.user }
            .flatMap {
                if (it != null) {
                    Result.success(it)
                } else {
                    Result.failure(Throwable("Cannot auth firebase"))
                }
            }
    }
}
