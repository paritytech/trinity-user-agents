package io.paritytech.polkadotapp.tools_backup_impl.data.store.backup

import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.api.client.googleapis.extensions.android.gms.auth.GoogleAccountCredential
import com.google.api.client.http.ByteArrayContent
import com.google.api.client.http.HttpResponseException
import com.google.api.client.http.javanet.NetHttpTransport
import com.google.api.client.json.gson.GsonFactory
import com.google.api.services.drive.Drive
import com.google.api.services.drive.DriveScopes
import com.google.api.services.drive.model.File
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.ResettableLazy
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.tools_auth_api.GoogleAuthManager
import io.paritytech.polkadotapp.tools_backup_api.domain.error.CorruptedBackupException
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupMetadata
import io.paritytech.polkadotapp.tools_backup_impl.BuildConfig
import io.paritytech.polkadotapp.tools_backup_impl.data.model.BackupConfig
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptedBackup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.RestoredEncryptedBackup
import java.io.ByteArrayOutputStream
import javax.inject.Inject

private const val FOLDER_NAME = "Polkadot App"
private const val APP_NAME = "Polkadotapp"

private const val BACKUP_MIME_TYPE = "application/json"
private const val TYPE_FOLDER = "application/vnd.google-apps.folder"
private const val DO_NOT_DELETE_FILE_POSTFIX = "DO_NOT_DELETE"
private const val DO_NOT_DELETE_FOLDER_POSTFIX = "DO NOT DELETE"

class GoogleDriveBackupStorage @Inject constructor(
    private val contextManager: ContextManager,
    private val googleManager: GoogleAuthManager,
) : EncryptedBackupStore {
    private val config = BackupConfig(FOLDER_NAME, APP_NAME)

    private val driveLazy = ResettableLazy { createGoogleDriveService() }

    private val drive by driveLazy

    override suspend fun isAuthorized(): Boolean = googleManager.isAuthorized()

    override suspend fun write(
        encryptedBackup: EncryptedBackup,
        metadata: BackupMetadata,
    ): Result<Unit> {
        return googleManager.runAuthenticated(driveScope()) {
            writeBackupFileToDrive(encryptedBackup.value, metadata.label)
        }
            .coerceToUnit()
    }

    override suspend fun read(): Result<RestoredEncryptedBackup?> {
        return googleManager.runAuthenticated(driveScope()) {
            readBackupFileFromDrive()
        }
    }

    override suspend fun delete(): Result<Unit> {
        if (!googleManager.isAuthorized()) return Result.success(Unit)
        return googleManager.runAuthenticated(driveScope()) {
            deleteBackupFileFromDrive()
        }
    }

    override suspend fun resetUserAuthentication() {
        googleManager.signOut()
        driveLazy.reset()
    }

    private fun writeBackupFileToDrive(fileContent: ByteArray, label: String): Result<File> {
        return runCatching {
            val contentStream = ByteArrayContent(BACKUP_MIME_TYPE, fileContent)

            val existing = getBackupFileFromCloud()

            if (existing != null) {
                drive.files().update(existing.id, File().apply { name = getFileName(label) }, contentStream).execute()
            } else {
                val fileMetadata = File().apply {
                    parents = listOf(getFolderId())
                    name = getFileName(label)
                }

                drive.files().create(fileMetadata, contentStream).execute()
            }
        }
    }

    private fun readBackupFileFromDrive(): Result<RestoredEncryptedBackup?> {
        return runCatching {
            val outputStream = ByteArrayOutputStream()

            val backupFile = getBackupFileFromCloud() ?: return@runCatching null

            try {
                drive.files()
                    .get(backupFile.id)
                    .executeMediaAndDownloadTo(outputStream)
            } catch (e: HttpResponseException) {
                // 416 means the stored backup is empty/unreadable — classify as corrupted at the detection site
                // https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/416
                if (e.statusCode == 416) throw CorruptedBackupException(e)
                throw e
            }

            RestoredEncryptedBackup(
                backup = EncryptedBackup(outputStream.toByteArray()),
                createdAt = backupFile.createdTime.value
            )
        }
    }

    private fun deleteBackupFileFromDrive(): Result<Unit> {
        return runCatching {
            val backupFile = getBackupFileFromCloud() ?: return@runCatching
            drive.moveToTrash(backupFile.id)
        }.coerceToUnit()
    }

    private fun getBackupFileFromCloud(): File? {
        return drive.files().list()
            .setQ(getFileQuery(getFolderId()))
            .setSpaces("drive")
            .setFields("files(id, name, createdTime)")
            .execute()
            .files
            .firstOrNull()
    }

    private fun createGoogleDriveService(): Drive {
        val context = contextManager.applicationContext
        val account = GoogleSignIn.getLastSignedInAccount(context)
        val credential = GoogleAccountCredential.usingOAuth2(context, listOf(driveScope()))
        credential.selectedAccount = account!!.account

        return Drive.Builder(NetHttpTransport(), GsonFactory(), credential)
            .setApplicationName("Polkadot")
            .build()
    }

    private fun driveScope(): String = DriveScopes.DRIVE_FILE

    private fun getFolderQuery() =
        "name = '${getFolderName()}' and mimeType = '$TYPE_FOLDER' and trashed = false"

    private fun createFolder() =
        drive.files().create(File().setName(getFolderName()).setMimeType(TYPE_FOLDER)).execute().id

    private fun getFolderId() =
        drive.files().list().setQ(getFolderQuery()).execute()?.files?.firstOrNull()?.id
            ?: createFolder()

    private fun getFolderName() = "${config.folderPrefix} - $DO_NOT_DELETE_FOLDER_POSTFIX"

    private fun getFilePrefix() =
        "${config.backupFilePrefix}-recovery-${BuildConfig.BACKUP_FILE_SUFFIX}"

    private fun getFileQuery(folderId: String) =
        "name contains '${getFilePrefix()}' and trashed = false and '$folderId' in parents"

    private fun getFileName(label: String) = "${getFilePrefix()}-$label-$DO_NOT_DELETE_FILE_POSTFIX"
}

private fun Drive.moveToTrash(fileId: String) =
    files().update(fileId, File().apply { trashed = true }).execute()
