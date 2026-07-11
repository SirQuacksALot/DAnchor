package com.danchor.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

enum class Tab { Connections, Profile }

/** Sits in the app's single persistent TopAppBar title slot (owned by
 * DAnchorApp, not by the individual pages) so switching tabs doesn't need a
 * separate bottom nav bar. `activeTab` is null while the Settings page is
 * showing - neither label is "active" then, since that page is represented
 * by the gear icon instead (see DAnchorApp.kt). */
@Composable
fun TabSwitcher(
    activeTab: Tab?,
    onTabChange: (Tab) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier) {
        TabLabel(stringResource(R.string.tab_connections), selected = activeTab == Tab.Connections) { onTabChange(Tab.Connections) }
        TabLabel(stringResource(R.string.tab_profile), selected = activeTab == Tab.Profile) { onTabChange(Tab.Profile) }
    }
}

@Composable
private fun TabLabel(
    text: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        color = if (selected) MaterialTheme.colorScheme.onSurface else mutedTextColor(),
        modifier =
            Modifier
                .clickable(onClick = onClick)
                .padding(end = 20.dp),
    )
}
