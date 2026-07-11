package com.danchor.app

import androidx.appcompat.app.AppCompatDelegate
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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.os.LocaleListCompat

/** The tag passed to [AppCompatDelegate.setApplicationLocales] - `null` means
 * "follow the system language" (an empty [LocaleListCompat]), otherwise a
 * BCP-47 language tag like "de"/"en". Language autonyms ("Deutsch"/"English")
 * are deliberately NOT translated resources - a language picker conventionally
 * shows each option in its own language regardless of the current UI
 * language, so a user can always find their way back. */
private enum class AppLanguage(val tag: String?, val label: String) {
    SYSTEM(null, "System"),
    GERMAN("de", "Deutsch"),
    ENGLISH("en", "English"),
}

@Composable
fun SettingsScreen(
    themeMode: ThemeMode,
    onThemeModeChange: (ThemeMode) -> Unit,
    autoReloadEnabled: Boolean,
    onAutoReloadEnabledChange: (Boolean) -> Unit,
    usbHintHidden: Boolean,
    onUsbHintHiddenChange: (Boolean) -> Unit,
    visibleForAll: Boolean,
    onVisibleForAllChange: (Boolean) -> Unit,
) {
    // AppCompatDelegate.setApplicationLocales() triggers an Activity
    // recreation to reload resources in the new language, so reading it
    // fresh at composition time (rather than mirroring it into its own
    // remembered state) is sufficient - there's no in-place recomposition
    // case to keep in sync.
    val currentLanguageTag = AppCompatDelegate.getApplicationLocales().toLanguageTags()
    val currentLanguage =
        AppLanguage.entries.find { it.tag != null && currentLanguageTag.startsWith(it.tag) } ?: AppLanguage.SYSTEM
    val onLanguageSelect: (AppLanguage) -> Unit = { language ->
        val locales =
            language.tag?.let { LocaleListCompat.forLanguageTags(it) }
                ?: LocaleListCompat.getEmptyLocaleList()
        AppCompatDelegate.setApplicationLocales(locales)
    }

    // Two independent Columns side by side (not a LazyVerticalGrid) - a grid
    // forces every cell in a row to the row's tallest height, leaving large
    // gaps under whichever section was shorter than its row-mate. A plain
    // Row of Columns lets each side pack its own sections tightly regardless
    // of the other side's height.
    Column(
        Modifier
            .fillMaxSize()
            .padding(horizontal = screenHorizontalPadding(), vertical = 16.dp)
            .verticalScroll(rememberScrollState()),
    ) {
        if (isTabletWidth()) {
            Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(24.dp)) {
                    DesignSection(themeMode, onThemeModeChange)
                    SwitchesSection(autoReloadEnabled, onAutoReloadEnabledChange, visibleForAll, onVisibleForAllChange)
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(24.dp)) {
                    LanguageSection(currentLanguage, onLanguageSelect)
                    UsbHintSection(usbHintHidden, onUsbHintHiddenChange)
                }
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(24.dp)) {
                DesignSection(themeMode, onThemeModeChange)
                LanguageSection(currentLanguage, onLanguageSelect)
                SwitchesSection(autoReloadEnabled, onAutoReloadEnabledChange, visibleForAll, onVisibleForAllChange)
                UsbHintSection(usbHintHidden, onUsbHintHiddenChange)
            }
        }
    }
}

@Composable
private fun DesignSection(
    themeMode: ThemeMode,
    onThemeModeChange: (ThemeMode) -> Unit,
) {
    Column {
        Text(stringResource(R.string.settings_design_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        Column(Modifier.selectableGroup()) {
            ThemeMode.entries.forEach { mode ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .selectable(
                            selected = themeMode == mode,
                            onClick = { onThemeModeChange(mode) },
                            role = Role.RadioButton,
                        )
                        .padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RadioButton(selected = themeMode == mode, onClick = null)
                    Spacer(Modifier.width(8.dp))
                    Text(themeModeLabel(mode))
                }
            }
        }
    }
}

@Composable
private fun LanguageSection(
    currentLanguage: AppLanguage,
    onLanguageSelect: (AppLanguage) -> Unit,
) {
    Column {
        Text(stringResource(R.string.language_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        Column(Modifier.selectableGroup()) {
            AppLanguage.entries.forEach { language ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .selectable(
                            selected = currentLanguage == language,
                            onClick = { onLanguageSelect(language) },
                            role = Role.RadioButton,
                        )
                        .padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RadioButton(selected = currentLanguage == language, onClick = null)
                    Spacer(Modifier.width(8.dp))
                    Text(language.label)
                }
            }
        }
    }
}

// Both switches share a single Column (no section title, just the two rows
// stacked directly on top of each other) - as two separate grid cells they
// each got stretched apart to match whatever taller cell happened to share
// their grid row, reading as two stranded controls instead of one panel.
@Composable
private fun SwitchesSection(
    autoReloadEnabled: Boolean,
    onAutoReloadEnabledChange: (Boolean) -> Unit,
    visibleForAll: Boolean,
    onVisibleForAllChange: (Boolean) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.auto_reload_label))
            Switch(checked = autoReloadEnabled, onCheckedChange = onAutoReloadEnabledChange)
        }
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.visible_for_all_label))
            Switch(checked = visibleForAll, onCheckedChange = onVisibleForAllChange)
        }
    }
}

@Composable
private fun UsbHintSection(
    usbHintHidden: Boolean,
    onUsbHintHiddenChange: (Boolean) -> Unit,
) {
    Column {
        Text(stringResource(R.string.usb_hint_settings_label), style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(4.dp))
        if (usbHintHidden) {
            Text(stringResource(R.string.usb_hint_hidden_body), color = mutedTextColor())
            Spacer(Modifier.height(8.dp))
            Button(onClick = { onUsbHintHiddenChange(false) }) {
                Text(stringResource(R.string.usb_hint_reshow_button))
            }
        } else {
            Text(stringResource(R.string.usb_hint_visible_body), color = mutedTextColor())
        }
    }
}

@Composable
private fun themeModeLabel(mode: ThemeMode): String =
    when (mode) {
        ThemeMode.LIGHT -> stringResource(R.string.theme_light)
        ThemeMode.DARK -> stringResource(R.string.theme_dark)
        ThemeMode.SYSTEM -> stringResource(R.string.option_system)
    }
