package dev.lysen.not

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.LargeTitleScreen
import dev.lysen.not.ui.LocalTabGestures
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.OnTabGesture
import dev.lysen.not.ui.TabGesture
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import java.time.LocalDate

/** What lives under the More tab. */
enum class MorePage { Budget, Workouts }

/** The More tab: a card per page, or the open page itself. */
@Composable
fun MoreScreen(page: MorePage?, onPage: (MorePage?) -> Unit, budget: BudgetViewModel, workouts: WorkoutsViewModel) {
    when (page) {
        MorePage.Budget -> BudgetScreen(budget, onBack = { onPage(null) })
        MorePage.Workouts -> WorkoutsScreen(workouts, onBack = { onPage(null) })
        null -> MoreHome(onPage, budget, workouts)
    }
}

@Composable
private fun MoreHome(onPage: (MorePage?) -> Unit, budget: BudgetViewModel, workouts: WorkoutsViewModel) {
    val finance by budget.state.collectAsStateWithLifecycle()
    val training by workouts.state.collectAsStateWithLifecycle()
    val gestures = LocalTabGestures.current
    // Holding More starts an entry in Budget.
    OnTabGesture { gesture ->
        if (gesture == TabGesture.Hold) {
            gestures.sendToNext(TabGesture.Hold)
            onPage(MorePage.Budget)
        }
    }
    LargeTitleScreen(title = "More") {
        item(key = "budget") {
            val b = finance.budget
            val line = when {
                b == null -> ""
                finance.accounts.isEmpty() -> "Accounts, limits and what you spend"
                b.available > 0 -> (if (b.left < 0) "${money(-b.left)} over" else "${money(b.left)} left") + " · ${b.daysLeft} ${if (b.daysLeft == 1u) "day" else "days"}"
                else -> "${money(b.spent)} spent this period"
            }
            val extra = listOfNotNull(
                finance.due.size.takeIf { it > 0 }?.let { "$it due" },
                b?.toReview?.takeIf { it > 0u }?.let { "$it to review" },
            ).joinToString(" · ")
            PageCard(R.drawable.ic_wallet, "Budget", line, extra) { onPage(MorePage.Budget) }
        }
        item(key = "workouts") {
            val monday = training.today.minusDays(training.today.dayOfWeek.value - 1L)
            val week = training.workouts.count { w ->
                w.exerciseNames.isNotEmpty() && runCatching { !LocalDate.parse(w.date).isBefore(monday) }.getOrDefault(false)
            }
            val last = training.workouts.firstOrNull { it.exerciseNames.isNotEmpty() }
            PageCard(
                R.drawable.ic_fitness,
                "Workouts",
                if (training.workouts.isEmpty()) "Sessions, sets and progress" else "$week this week",
                last?.let { "Last: ${dayLabel(it.date, training.today)}" }.orEmpty(),
            ) { onPage(MorePage.Workouts) }
        }
    }
}

@Composable
private fun PageCard(icon: Int, title: String, line: String, extra: String, onClick: () -> Unit) {
    val colors = Not.colors
    Row(
        Modifier
            .padding(horizontal = 16.dp, vertical = 5.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(22.dp))
            .background(colors.surface)
            .pressable(onClick, pressedScale = 0.98f)
            .padding(horizontal = 18.dp, vertical = 18.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(44.dp).clip(CircleShape).background(colors.accentSoft), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = colors.accentInk)
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = Not.type.heading)
            if (line.isNotEmpty()) Text(line, style = Not.type.body, color = colors.inkMuted, maxLines = 1)
            if (extra.isNotEmpty()) Text(extra, style = Not.type.caption, color = colors.accentInk, maxLines = 1)
        }
    }
}
