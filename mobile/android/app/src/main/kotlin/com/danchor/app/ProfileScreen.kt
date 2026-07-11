package com.danchor.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

@Composable
fun ProfileScreen(
    deviceProfileName: String,
    onDeviceProfileNameChange: (String) -> Unit,
    deviceId: String,
    pairingSecret: String,
    onRegenerateSecret: () -> Unit,
    onAdoptSecret: (String) -> Unit,
) {
    var name by remember(deviceProfileName) { mutableStateOf(deviceProfileName) }
    var secretVisible by remember { mutableStateOf(false) }
    var showRegenerateConfirm by remember { mutableStateOf(false) }
    var pastedSecret by remember { mutableStateOf("") }
    val clipboardManager = LocalClipboardManager.current

    // Two independent Columns side by side (not a LazyVerticalGrid) - a grid
    // forces every cell in a row to the row's tallest height, which left
    // large gaps under the shorter section whenever its row-mate (e.g. Trust-
    // Secret, several lines taller than Gerätename) was taller. A plain Row
    // of Columns lets each side pack its own sections tightly regardless of
    // the other side's height. Device identity (id, name) groups in one
    // column, secret-related settings in the other - Geräte-ID comes before
    // Gerätename per Sebastian's explicit ordering request.
    Column(
        Modifier
            .fillMaxSize()
            .padding(horizontal = screenHorizontalPadding(), vertical = 16.dp)
            .verticalScroll(rememberScrollState()),
    ) {
        if (isTabletWidth()) {
            Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(24.dp)) {
                    DeviceIdSection(deviceId, onCopyDeviceId = { clipboardManager.setText(AnnotatedString(deviceId)) })
                    DeviceNameSection(name) {
                        name = it
                        onDeviceProfileNameChange(it)
                    }
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(24.dp)) {
                    TrustSecretSection(
                        pairingSecret = pairingSecret,
                        secretVisible = secretVisible,
                        onToggleVisible = { secretVisible = !secretVisible },
                        onCopySecret = { clipboardManager.setText(AnnotatedString(pairingSecret)) },
                        onRegenerateClick = { showRegenerateConfirm = true },
                    )
                    AdoptSecretSection(
                        pastedSecret = pastedSecret,
                        onPastedSecretChange = { pastedSecret = it },
                        onAdopt = {
                            onAdoptSecret(pastedSecret)
                            pastedSecret = ""
                        },
                    )
                }
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(24.dp)) {
                DeviceIdSection(deviceId, onCopyDeviceId = { clipboardManager.setText(AnnotatedString(deviceId)) })
                DeviceNameSection(name) {
                    name = it
                    onDeviceProfileNameChange(it)
                }
                TrustSecretSection(
                    pairingSecret = pairingSecret,
                    secretVisible = secretVisible,
                    onToggleVisible = { secretVisible = !secretVisible },
                    onCopySecret = { clipboardManager.setText(AnnotatedString(pairingSecret)) },
                    onRegenerateClick = { showRegenerateConfirm = true },
                )
                AdoptSecretSection(
                    pastedSecret = pastedSecret,
                    onPastedSecretChange = { pastedSecret = it },
                    onAdopt = {
                        onAdoptSecret(pastedSecret)
                        pastedSecret = ""
                    },
                )
            }
        }
    }

    if (showRegenerateConfirm) {
        AlertDialog(
            onDismissRequest = { showRegenerateConfirm = false },
            title = { Text(stringResource(R.string.regenerate_confirm_title)) },
            text = { Text(stringResource(R.string.regenerate_confirm_body)) },
            confirmButton = {
                TextButton(onClick = {
                    onRegenerateSecret()
                    showRegenerateConfirm = false
                }) {
                    Text(stringResource(R.string.regenerate_button))
                }
            },
            dismissButton = {
                TextButton(onClick = { showRegenerateConfirm = false }) {
                    Text(stringResource(R.string.cancel_button))
                }
            },
        )
    }
}

@Composable
private fun DeviceIdSection(
    deviceId: String,
    onCopyDeviceId: () -> Unit,
) {
    Column {
        Text(stringResource(R.string.device_id_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                deviceId,
                style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                color = mutedTextColor(),
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onCopyDeviceId) {
                Text(stringResource(R.string.copy_button))
            }
        }
    }
}

@Composable
private fun DeviceNameSection(
    name: String,
    onNameChange: (String) -> Unit,
) {
    Column {
        Text(stringResource(R.string.device_name_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        TextField(
            value = name,
            onValueChange = onNameChange,
            shape = RectangleShape,
            colors = transparentTextFieldColors(),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun TrustSecretSection(
    pairingSecret: String,
    secretVisible: Boolean,
    onToggleVisible: () -> Unit,
    onCopySecret: () -> Unit,
    onRegenerateClick: () -> Unit,
) {
    Column {
        Text(stringResource(R.string.trust_secret_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        Text(
            stringResource(R.string.trust_secret_description),
            style = MaterialTheme.typography.bodySmall,
            color = mutedTextColor(),
        )
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (secretVisible) pairingSecret else "•".repeat(16),
                style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onToggleVisible) {
                Text(stringResource(if (secretVisible) R.string.hide_button else R.string.reveal_button))
            }
        }
        Spacer(Modifier.height(8.dp))
        Row {
            Button(onClick = onCopySecret) {
                Text(stringResource(R.string.copy_button))
            }
            Spacer(Modifier.width(8.dp))
            Button(onClick = onRegenerateClick) {
                Text(stringResource(R.string.regenerate_button))
            }
        }
    }
}

@Composable
private fun AdoptSecretSection(
    pastedSecret: String,
    onPastedSecretChange: (String) -> Unit,
    onAdopt: () -> Unit,
) {
    Column {
        Text(stringResource(R.string.adopt_secret_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        TextField(
            value = pastedSecret,
            onValueChange = onPastedSecretChange,
            label = { Text(stringResource(R.string.paste_secret_label)) },
            shape = RectangleShape,
            colors = transparentTextFieldColors(),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        Button(onClick = onAdopt, enabled = pastedSecret.isNotBlank()) {
            Text(stringResource(R.string.adopt_button))
        }
    }
}
