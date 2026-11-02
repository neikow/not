package dev.lysen.not.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.Stable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Room the floating tab bar takes at the bottom (0 where it's hidden); lists pad by it. */
val LocalBottomBarHeight = compositionLocalOf { 0.dp }

private val TopBarHeight = 52.dp

/**
 * A screen with a big title that scrolls with the content; once it's scrolled away, its name
 * fades into a slim bar at the top. [header] goes under the title (subtitles, chips).
 */
@Composable
fun LargeTitleScreen(
    title: String,
    modifier: Modifier = Modifier,
    state: LazyListState = rememberLazyListState(),
    onTitleClick: (() -> Unit)? = null,
    titleAccessory: (@Composable () -> Unit)? = null,
    navigation: (@Composable RowScope.() -> Unit)? = null,
    actions: (@Composable RowScope.() -> Unit)? = null,
    header: (@Composable () -> Unit)? = null,
    bottomPadding: Dp = LocalBottomBarHeight.current,
    content: LazyListScope.() -> Unit,
) {
    val colors = Not.colors
    val collapsed by remember { derivedStateOf { state.firstVisibleItemIndex > 0 || state.firstVisibleItemScrollOffset > 90 } }
    val barAlpha by animateFloatAsState(if (collapsed) 1f else 0f, label = "bar")
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    Box(modifier.fillMaxSize().background(colors.background)) {
        LazyColumn(
            Modifier.fillMaxSize(),
            state = state,
            contentPadding = PaddingValues(top = top + TopBarHeight, bottom = bottomPadding + 24.dp),
        ) {
            item(key = "title") {
                Column(Modifier.padding(horizontal = 24.dp).padding(top = 4.dp, bottom = 12.dp)) {
                    Row(
                        Modifier.then(if (onTitleClick != null) Modifier.pressable(onTitleClick, pressedScale = 0.98f) else Modifier),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(title, style = Not.type.display, maxLines = 1, modifier = Modifier.weight(1f, fill = false))
                        titleAccessory?.invoke()
                    }
                    header?.invoke()
                }
            }
            content()
        }
        // The slim bar: always holds the buttons; the title and a hairline appear once scrolled.
        Column(Modifier.fillMaxWidth().background(colors.background.copy(alpha = barAlpha)).statusBarsPadding()) {
            Row(
                Modifier.fillMaxWidth().height(TopBarHeight).padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) { navigation?.invoke(this) }
                Text(title, style = Not.type.heading, maxLines = 1, modifier = Modifier.alpha(barAlpha).padding(horizontal = 8.dp))
                Row(Modifier.weight(1f), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                    actions?.invoke(this)
                }
            }
            Divider(Modifier.alpha(barAlpha))
        }
    }
}

class TabItem(val label: String, val icon: Int, val badge: Int = 0)

/**
 * The floating pill at the bottom. The selected tab grows into a mint capsule with its label;
 * the others are just icons.
 */
@Composable
fun FloatingTabBar(tabs: List<TabItem>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    val colors = Not.colors
    Row(
        modifier
            .shadow(18.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.18f), spotColor = Color.Black.copy(alpha = 0.18f))
            .clip(CircleShape)
            .background(colors.surface)
            .padding(6.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        tabs.forEachIndexed { index, tab ->
            val isSelected = index == selected
            val bg by animateColorAsState(if (isSelected) colors.accent else Color.Transparent, label = "tab")
            Row(
                Modifier
                    .clip(CircleShape)
                    .background(bg)
                    .pressable({ onSelect(index) }, pressedScale = 0.92f, onClickLabel = tab.label)
                    .animateContentSize(spring(dampingRatio = 0.8f, stiffness = Spring.StiffnessMediumLow))
                    .height(48.dp)
                    .padding(horizontal = if (isSelected) 18.dp else 16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Box {
                    Icon(tab.icon, if (isSelected) null else tab.label, tint = if (isSelected) colors.onAccent else colors.inkMuted)
                    if (tab.badge > 0 && !isSelected) {
                        Badge(tab.badge, Modifier.align(Alignment.TopEnd).offset(x = 11.dp, y = (-8).dp))
                    }
                }
                if (isSelected) Text(tab.label, style = Not.type.label, color = colors.onAccent, maxLines = 1)
            }
        }
    }
}

/** The tab bar and, when the screen has one, its + button, centred together above the system bar. */
@Composable
fun BottomBar(tabs: List<TabItem>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    val add = LocalAddAction.current
    Row(
        modifier.padding(bottom = 12.dp + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FloatingTabBar(tabs, selected, onSelect)
        val action = add.action
        androidx.compose.animation.AnimatedVisibility(
            action != null,
            enter = androidx.compose.animation.scaleIn(spring(0.6f)) + androidx.compose.animation.fadeIn(),
            exit = androidx.compose.animation.scaleOut() + androidx.compose.animation.fadeOut(),
        ) {
            FloatingAddButton(add.label, { add.action?.invoke() })
        }
    }
}

/** Height the tab bar occupies, including its bottom gap and the system navigation bar. */
@Composable
fun floatingTabBarHeight(): Dp = 60.dp + 12.dp + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()

/** What the + beside the tab bar does on the current screen; screens set it with [AddAction]. */
@Stable
class AddActionHolder {
    var label by mutableStateOf("")
    var action by mutableStateOf<(() -> Unit)?>(null)
}

val LocalAddAction = compositionLocalOf { AddActionHolder() }

/** Shows the floating + while this is composed and [enabled]; tapping it runs [onClick]. */
@Composable
fun AddAction(label: String, enabled: Boolean = true, onClick: () -> Unit) {
    val holder = LocalAddAction.current
    val latest by rememberUpdatedState(onClick)
    DisposableEffect(holder, label, enabled) {
        holder.label = label
        holder.action = if (enabled) ({ latest() }) else null
        onDispose { holder.action = null }
    }
}

/** The round mint + that sits beside the tab bar. */
@Composable
fun FloatingAddButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val colors = Not.colors
    Box(
        modifier
            .size(60.dp)
            .shadow(18.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.18f), spotColor = Color.Black.copy(alpha = 0.18f))
            .clip(CircleShape)
            .background(colors.accent)
            .pressable(onClick, pressedScale = 0.9f, onClickLabel = label),
        contentAlignment = Alignment.Center,
    ) {
        Icon(dev.lysen.not.R.drawable.ic_add, label, tint = colors.onAccent, size = 28.dp)
    }
}
