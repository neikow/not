package dev.lysen.not

import android.app.DatePickerDialog
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.OutputTransformation
import androidx.compose.foundation.text.input.TextFieldBuffer
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.AccountKind
import dev.lysen.not.core.CategoryBudget
import dev.lysen.not.core.CategoryKind
import dev.lysen.not.core.MoneyAccount
import dev.lysen.not.core.MoneyBudget
import dev.lysen.not.core.MoneyCategory
import dev.lysen.not.core.MoneyRecurring
import dev.lysen.not.core.MoneyTokenKind
import dev.lysen.not.core.MoneyTxn
import dev.lysen.not.core.ParsedMoney
import dev.lysen.not.core.RecurringInput
import dev.lysen.not.core.Rollover
import dev.lysen.not.core.TxnFilter
import dev.lysen.not.core.TxnInput
import dev.lysen.not.core.TxnKind
import dev.lysen.not.core.formatMoney
import dev.lysen.not.core.parseCents
import dev.lysen.not.ui.AddAction
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Divider
import dev.lysen.not.ui.EmptyState
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.IconButton
import dev.lysen.not.ui.LargeTitleScreen
import dev.lysen.not.ui.LocalBottomBarHeight
import dev.lysen.not.ui.Menu
import dev.lysen.not.ui.MenuItem
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.NotColors
import dev.lysen.not.ui.OnTabGesture
import dev.lysen.not.ui.PromptDialog
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.SwipeAction
import dev.lysen.not.ui.SwipeRow
import dev.lysen.not.ui.TabGesture
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.ui.ToastState
import dev.lysen.not.ui.pressable
import java.time.LocalDate
import java.time.format.DateTimeFormatter

/** "12,50 €"; [signed] adds "+" to money in. */
fun money(cents: Long, signed: Boolean = false): String = formatMoney(cents, signed)

private val periodDay = DateTimeFormatter.ofPattern("d MMM")

private fun periodLabel(b: MoneyBudget): String {
    val (s, e) = LocalDate.parse(b.start) to LocalDate.parse(b.end)
    return "${s.format(periodDay)} – ${e.format(periodDay)}"
}

/** Budget, with the transactions page, entry sheet and the editors opening over it. */
@Composable
fun BudgetScreen(viewModel: BudgetViewModel, onBack: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toasts = remember { ToastState() }
    var entering by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<MoneyTxn?>(null) }
    var drafting by remember { mutableStateOf<TxnInput?>(null) }
    var sheet by remember { mutableStateOf<BudgetSheet?>(null) }

    LaunchedEffect(Unit) {
        viewModel.undoables.collect { if (toasts.show(it.message, action = "Undo")) viewModel.undo() }
    }
    LaunchedEffect(Unit) { viewModel.refreshToday() }

    // Tapping More again closes what's open, then comes back to this period, then leaves; holding
    // it starts an entry.
    OnTabGesture { gesture ->
        when {
            gesture == TabGesture.Hold -> entering = true
            state.txnView != null -> viewModel.openTxns(null)
            !state.showsCurrentPeriod -> viewModel.showCurrentPeriod()
            else -> onBack()
        }
    }

    Box(Modifier.fillMaxSize()) {
        val view = state.txnView
        if (view != null) {
            BackHandler { viewModel.openTxns(null) }
            TransactionsPage(view, state, viewModel, onOpen = { editing = it })
        } else {
            BackHandler(onBack = onBack)
            BudgetPage(state, viewModel, onBack, onSheet = { sheet = it })
        }
        AddAction(
            if (state.accounts.isEmpty()) "Add an account" else "Add an entry",
            holdLabel = "Add with details",
            onHold = { if (state.accounts.isNotEmpty()) drafting = blankDraft(state, viewModel) else sheet = BudgetSheet.Accounts },
        ) { if (state.accounts.isEmpty()) sheet = BudgetSheet.Accounts else entering = true }
        ToastHost(toasts, Modifier.align(Alignment.BottomCenter).imePadding().padding(bottom = LocalBottomBarHeight.current + 8.dp))
    }

    if (entering) {
        EntrySheet(state, viewModel, onDismiss = { entering = false }, onDetails = {
            entering = false
            drafting = it
        })
    }
    editing?.let { txn -> TxnSheet(state, viewModel, txn.toInput(), txn, onDismiss = { editing = null }) }
    drafting?.let { draft -> TxnSheet(state, viewModel, draft, null, onDismiss = { drafting = null }) }
    when (sheet) {
        BudgetSheet.Accounts -> AccountsSheet(state, viewModel) { sheet = null }
        BudgetSheet.Categories -> CategoriesSheet(state, viewModel) { sheet = null }
        BudgetSheet.Recurring -> RecurringSheet(state, viewModel) { sheet = null }
        BudgetSheet.PeriodDay -> PromptDialog(
            title = "Periods start on day",
            confirm = "Save",
            initial = state.settings.periodStartDay.toString(),
            placeholder = "1–31, e.g. your payday",
            onConfirm = { text ->
                text.toIntOrNull()?.takeIf { it in 1..31 }?.let(viewModel::setPeriodStartDay)
                sheet = null
            },
            onDismiss = { sheet = null },
        )
        null -> {}
    }
    state.error?.let {
        CardDialog(title = "Something went wrong", onDismiss = viewModel::dismissError, buttons = { Button("OK", viewModel::dismissError) }) {
            Text(it, color = Not.colors.inkMuted)
        }
    }
}

private enum class BudgetSheet { Accounts, Categories, Recurring, PeriodDay }

private fun blankDraft(state: BudgetState, viewModel: BudgetViewModel): TxnInput {
    val parsed = viewModel.parse("0")
    return TxnInput(TxnKind.ENTRY, parsed?.account ?: state.accounts.first().id, null, state.today.toString(), 0, "", "", null, emptyList())
}

private fun MoneyTxn.toInput() = TxnInput(kind, account, toAccount, date, amount, payee, note, category, splits)

// ---- budget page ---------------------------------------------------------------------------------

@Composable
private fun BudgetPage(state: BudgetState, viewModel: BudgetViewModel, onBack: () -> Unit, onSheet: (BudgetSheet) -> Unit) {
    val b = state.budget
    var menu by remember { mutableStateOf(false) }
    fun open(filter: TxnFilter, title: String) = viewModel.openTxns(TxnView(filter, title))
    val period = b?.let { TxnFilter(null, null, it.start, it.end, false) }
    LargeTitleScreen(
        title = "Budget",
        navigation = { IconButton(R.drawable.ic_arrow_back, "Back", onBack) },
        actions = {
            IconButton(R.drawable.ic_list, "Transactions", { open(TxnFilter(null, null, null, null, false), "Transactions") })
            Box {
                IconButton(R.drawable.ic_more_vert, "More", { menu = true })
                Menu(menu, onDismiss = { menu = false }) {
                    MenuItem("Accounts", icon = R.drawable.ic_wallet, onClick = { menu = false; onSheet(BudgetSheet.Accounts) })
                    MenuItem("Categories", icon = R.drawable.ic_folder, onClick = { menu = false; onSheet(BudgetSheet.Categories) })
                    MenuItem("Recurring", icon = R.drawable.ic_repeat, onClick = { menu = false; onSheet(BudgetSheet.Recurring) })
                    MenuItem("Period start day", icon = R.drawable.ic_calendar, onClick = { menu = false; onSheet(BudgetSheet.PeriodDay) })
                }
            }
        },
        onSwipe = viewModel::step,
        header = { if (b != null) BudgetHeader(b, state, viewModel, onReview = { open(TxnFilter(null, null, b.start, b.end, true), "To review") }) },
    ) {
        if (b == null) return@LargeTitleScreen
        if (state.accounts.isEmpty()) {
            item(key = "empty") {
                EmptyState(R.drawable.ic_wallet, "Start with an account", "Add your bank accounts and cash, then note what you spend with + — like “12,50 coffee #resto”.")
            }
            item(key = "add-account") {
                Box(Modifier.fillMaxWidth().padding(top = 20.dp), contentAlignment = Alignment.Center) {
                    Button("Add an account", { onSheet(BudgetSheet.Accounts) }, icon = R.drawable.ic_add)
                }
            }
            return@LargeTitleScreen
        }
        if (state.due.isNotEmpty() && state.showsCurrentPeriod) {
            item(key = "due-title") { SectionTitle("Due") }
            item(key = "due") {
                Group {
                    state.due.forEachIndexed { i, r ->
                        if (i > 0) RowDivider()
                        DueRow(r, state, viewModel)
                    }
                }
            }
        }
        item(key = "spending-title") { SectionTitle("Spending") }
        item(key = "spending") {
            Group {
                b.expenses.forEachIndexed { i, line ->
                    if (i > 0) RowDivider()
                    CategoryRow(line) { period?.let { open(it.copy(category = line.category.id), "${line.category.emoji} ${line.category.name}".trim()) } }
                }
                if (b.unsortedSpent > 0 || b.toReview > 0u) {
                    RowDivider()
                    UnsortedRow(b) { open(TxnFilter(null, null, b.start, b.end, true), "To review") }
                }
            }
        }
        if (b.income.isNotEmpty() || b.unsortedIncome > 0) {
            item(key = "income-title") { SectionTitle("Income") }
            item(key = "income") {
                Group {
                    b.income.forEachIndexed { i, line ->
                        if (i > 0) RowDivider()
                        IncomeRow(line) { period?.let { open(it.copy(category = line.category.id), line.category.name) } }
                    }
                }
            }
        }
        item(key = "totals") { Totals(b) }
    }
}

@Composable
private fun BudgetHeader(b: MoneyBudget, state: BudgetState, viewModel: BudgetViewModel, onReview: () -> Unit) {
    val colors = Not.colors
    val current = state.showsCurrentPeriod
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                periodLabel(b) + if (current && b.daysLeft > 0u) " · ${b.daysLeft} ${if (b.daysLeft == 1u) "day" else "days"} left" else "",
                style = Not.type.body,
                color = colors.inkMuted,
                modifier = Modifier.weight(1f),
            )
            if (!current) Button("This period", viewModel::showCurrentPeriod, kind = ButtonKind.Quiet)
        }
        if (b.available > 0) {
            Row(verticalAlignment = Alignment.Bottom) {
                Text(money(b.left), style = Not.type.display.merge(fontSize = 40.sp, lineHeight = 46.sp), color = if (b.left < 0) colors.danger else colors.ink)
                Text(if (b.left < 0) "  over" else "  left", style = Not.type.bodyStrong, color = colors.inkMuted, modifier = Modifier.padding(bottom = 6.dp))
            }
            Bar(b.spent, b.available, Modifier.fillMaxWidth())
            Text("${money(b.spent)} spent of ${money(b.available)}", style = Not.type.caption, color = colors.inkFaint)
        } else {
            Row(verticalAlignment = Alignment.Bottom) {
                Text(money(b.spent), style = Not.type.display.merge(fontSize = 40.sp, lineHeight = 46.sp))
                Text("  spent", style = Not.type.bodyStrong, color = colors.inkMuted, modifier = Modifier.padding(bottom = 6.dp))
            }
            Text("Set limits in Categories to see what's left.", style = Not.type.caption, color = colors.inkFaint)
        }
        if (b.toReview > 0u) {
            Row(Modifier.padding(top = 4.dp)) {
                Chip(R.drawable.ic_inbox, "${b.toReview} to review", onClick = onReview)
            }
        }
    }
}

/** Spent against available: mint, or red past the end. */
@Composable
private fun Bar(spent: Long, available: Long, modifier: Modifier = Modifier, height: Int = 6) {
    val colors = Not.colors
    val target = if (available <= 0) (if (spent > 0) 1f else 0f) else (spent.toFloat() / available).coerceIn(0f, 1f)
    val fraction by animateFloatAsState(target, label = "bar")
    val over = available > 0 && spent > available || available <= 0 && spent > 0 && height > 6
    Box(modifier.height(height.dp).clip(CircleShape).background(colors.sunken)) {
        Box(Modifier.fillMaxWidth(fraction).height(height.dp).clip(CircleShape).background(if (over) colors.danger else colors.accent))
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(text, style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 28.dp, end = 28.dp, top = 18.dp, bottom = 8.dp))
}

@Composable
private fun Group(content: @Composable () -> Unit) {
    Column(
        Modifier
            .padding(horizontal = 16.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(22.dp))
            .background(Not.colors.surface),
    ) { content() }
}

@Composable
private fun RowDivider() = Divider(Modifier.padding(start = 66.dp))

@Composable
private fun Emoji(emoji: String, tint: Color? = null) {
    Box(Modifier.size(36.dp).clip(CircleShape).background(tint ?: Not.colors.sunken), contentAlignment = Alignment.Center) {
        Text(emoji.ifEmpty { "•" }, style = TextStyle(fontSize = 18.sp), textAlign = TextAlign.Center)
    }
}

@Composable
private fun CategoryRow(line: CategoryBudget, onClick: () -> Unit) {
    val colors = Not.colors
    val available = line.limit + line.carried
    Row(
        Modifier.fillMaxWidth().pressable(onClick, pressedScale = 0.99f).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Emoji(line.category.emoji)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(line.category.name, style = Not.type.bodyStrong, maxLines = 1, modifier = Modifier.weight(1f))
                when {
                    available > 0 || line.limit > 0 -> Text(
                        if (line.left < 0) "${money(-line.left)} over" else "${money(line.left)} left",
                        style = Not.type.label,
                        color = if (line.left < 0) colors.danger else colors.ink,
                    )
                    line.spent != 0L -> Text(money(line.spent), style = Not.type.label)
                }
            }
            if (available > 0 || line.limit > 0) Bar(line.spent, available, Modifier.fillMaxWidth(), height = 4)
            val detail = buildList {
                if (available > 0 || line.limit > 0) add("${money(line.spent)} of ${money(available)}")
                else add(if (line.spent == 0L) "Nothing yet" else "No limit")
                if (line.carried != 0L) add("${money(line.carried, signed = true)} carried")
            }.joinToString(" · ")
            Text(detail, style = Not.type.caption, color = colors.inkFaint, maxLines = 1)
        }
    }
}

@Composable
private fun UnsortedRow(b: MoneyBudget, onClick: () -> Unit) {
    val colors = Not.colors
    Row(
        Modifier.fillMaxWidth().pressable(onClick, pressedScale = 0.99f).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Emoji("?", tint = colors.accentSoft)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text("No category", style = Not.type.bodyStrong)
            Text(if (b.toReview > 0u) "${b.toReview} to sort" else "From deleted categories", style = Not.type.caption, color = colors.accentInk)
        }
        Text(money(b.unsortedSpent), style = Not.type.label)
    }
}

@Composable
private fun IncomeRow(line: CategoryBudget, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().pressable(onClick, pressedScale = 0.99f).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Emoji(line.category.emoji)
        Spacer(Modifier.width(16.dp))
        Text(line.category.name, style = Not.type.bodyStrong, modifier = Modifier.weight(1f))
        Text(money(line.spent, signed = true), style = Not.type.label, color = if (line.spent > 0) Not.colors.accentInk else Not.colors.inkFaint)
    }
}

@Composable
private fun DueRow(r: MoneyRecurring, state: BudgetState, viewModel: BudgetViewModel) {
    val colors = Not.colors
    var other by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier.fillMaxWidth().padding(start = 14.dp, end = 8.dp, top = 10.dp, bottom = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Emoji(state.category(r.category)?.emoji ?: "🔁")
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f).pressable({ menu = true }, pressedScale = 0.99f)) {
                Text(r.name, style = Not.type.bodyStrong, maxLines = 1)
                Text("${dayLabel(r.nextDue, state.today)} · ${money(r.amount, signed = true)}", style = Not.type.caption, color = colors.inkFaint, maxLines = 1)
            }
            Button("Enter", { viewModel.enterRecurring(r) })
        }
        Menu(menu, onDismiss = { menu = false }) {
            MenuItem("Enter another amount", icon = R.drawable.ic_edit, onClick = { menu = false; other = true })
            MenuItem("Skip this time", icon = R.drawable.ic_close, onClick = { menu = false; viewModel.skipRecurring(r) })
        }
    }
    if (other) {
        PromptDialog(
            title = r.name,
            confirm = "Enter",
            initial = formatPlain(-r.amount),
            placeholder = "Amount",
            onConfirm = { text ->
                parseCents(text)?.let { viewModel.enterRecurring(r, if (r.amount < 0) -kotlin.math.abs(it) else kotlin.math.abs(it)) }
                other = false
            },
            onDismiss = { other = false },
        )
    }
}

@Composable
private fun Totals(b: MoneyBudget) {
    val colors = Not.colors
    Row(Modifier.fillMaxWidth().padding(horizontal = 28.dp, vertical = 20.dp), horizontalArrangement = Arrangement.SpaceBetween) {
        listOf("In" to money(b.incomeTotal), "Out" to money(b.spent), "Net" to money(b.net, signed = true)).forEach { (label, value) ->
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(label, style = Not.type.caption, color = colors.inkFaint)
                Text(value, style = Not.type.label, color = if (label == "Net" && b.net < 0) colors.danger else colors.ink)
            }
        }
    }
}

// ---- transactions --------------------------------------------------------------------------------

@Composable
private fun TransactionsPage(view: TxnView, state: BudgetState, viewModel: BudgetViewModel, onOpen: (MoneyTxn) -> Unit) {
    val byDay = state.txns.groupBy { it.date }
    LargeTitleScreen(
        title = view.title,
        navigation = { IconButton(R.drawable.ic_arrow_back, "Back", { viewModel.openTxns(null) }) },
        header = { TxnFilters(view, state, viewModel) },
    ) {
        if (state.txns.isEmpty()) {
            item(key = "empty") {
                EmptyState(R.drawable.ic_list, if (view.filter.toReview) "All sorted" else "Nothing here", if (view.filter.toReview) "Every entry has a category." else "Entries you add show up here, newest first.")
            }
        }
        byDay.forEach { (day, rows) ->
            item(key = "day-$day") {
                Row(Modifier.padding(start = 28.dp, end = 28.dp, top = 16.dp, bottom = 6.dp)) {
                    Text(dayLabel(day, state.today, long = true), style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.weight(1f))
                    val net = rows.filter { it.kind == TxnKind.ENTRY }.sumOf { it.amount }
                    if (net != 0L) Text(money(net, signed = true), style = Not.type.caption, color = Not.colors.inkFaint)
                }
            }
            item(key = "rows-$day") {
                Group {
                    rows.forEachIndexed { i, t ->
                        if (i > 0) RowDivider()
                        TxnRow(t, state, viewModel, onOpen)
                    }
                }
            }
        }
    }
}

@Composable
private fun TxnFilters(view: TxnView, state: BudgetState, viewModel: BudgetViewModel) {
    val f = view.filter
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        FilterChip("All accounts", f.account == null) { viewModel.openTxns(view.copy(filter = f.copy(account = null))) }
        state.accounts.forEach { a ->
            FilterChip(a.name, f.account == a.id) { viewModel.openTxns(view.copy(filter = f.copy(account = if (f.account == a.id) null else a.id))) }
        }
        if (f.category != null) {
            FilterChip("Any category", false) { viewModel.openTxns(TxnView(f.copy(category = null), "Transactions")) }
        }
        FilterChip("To review", f.toReview) { viewModel.openTxns(view.copy(filter = f.copy(toReview = !f.toReview))) }
    }
}

@Composable
private fun FilterChip(label: String, selected: Boolean, onClick: () -> Unit) {
    val colors = Not.colors
    Box(
        Modifier
            .clip(CircleShape)
            .background(if (selected) colors.accentSoft else colors.sunken)
            .pressable(onClick, pressedScale = 0.96f)
            .heightIn(min = 34.dp)
            .padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, style = Not.type.caption.merge(color = if (selected) colors.accentInk else colors.inkMuted), maxLines = 1)
    }
}

@Composable
private fun TxnRow(t: MoneyTxn, state: BudgetState, viewModel: BudgetViewModel, onOpen: (MoneyTxn) -> Unit) {
    val colors = Not.colors
    val haptics = LocalHapticFeedback.current
    val (emoji, title, detail) = txnLook(t, state)
    SwipeRow(
        start = null,
        end = SwipeAction(R.drawable.ic_delete, colors.danger, Color.White, dismisses = true) {
            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            viewModel.delete(t)
        },
    ) {
        Row(
            Modifier.fillMaxWidth().background(colors.surface).pressable({ onOpen(t) }, pressedScale = 0.99f).padding(horizontal = 14.dp, vertical = 11.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Emoji(emoji, tint = if (t.needsReview) colors.accentSoft else null)
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                Text(title, style = Not.type.bodyStrong, maxLines = 1)
                Text(detail, style = Not.type.caption, color = if (t.needsReview) colors.accentInk else colors.inkFaint, maxLines = 1)
            }
            Spacer(Modifier.width(8.dp))
            Text(
                if (t.kind == TxnKind.TRANSFER) money(t.amount) else money(t.amount, signed = true),
                style = Not.type.label,
                color = when {
                    t.kind != TxnKind.ENTRY -> colors.inkMuted
                    t.amount > 0 -> colors.accentInk
                    else -> colors.ink
                },
            )
        }
    }
}

/** (emoji, title, detail line) for a transaction row. */
private fun txnLook(t: MoneyTxn, state: BudgetState): Triple<String, String, String> {
    val account = state.account(t.account)?.name ?: "?"
    return when (t.kind) {
        TxnKind.TRANSFER -> Triple("⇄", t.payee.ifEmpty { "Transfer" }, "$account → ${state.account(t.toAccount)?.name ?: "?"}")
        TxnKind.ADJUSTMENT -> Triple("±", "Balance set", account)
        TxnKind.ENTRY -> {
            val category = state.category(t.category)
            val splitNames = t.splits.mapNotNull { state.category(it.category)?.name }
            val emoji = category?.emoji ?: t.splits.firstOrNull()?.let { state.category(it.category)?.emoji } ?: "?"
            val title = t.payee.ifEmpty { category?.name ?: "Entry" }
            val what = when {
                t.splits.isNotEmpty() -> splitNames.joinToString(" + ")
                category != null -> category.name
                else -> "Tap to pick a category"
            }
            Triple(emoji, title, "$what · $account")
        }
    }
}

// ---- quick entry ---------------------------------------------------------------------------------

/** Tints what the parser recognised. */
private class MoneyHighlight(private val parsed: () -> ParsedMoney?, private val colors: NotColors) : OutputTransformation {
    override fun TextFieldBuffer.transformOutput() {
        for (token in parsed()?.tokens.orEmpty()) {
            val (start, end) = token.start.toInt() to token.end.toInt()
            if (end <= length) addStyle(SpanStyle(color = colors.accentInk, background = colors.accentSoft), start, end)
        }
    }
}

/** One line, like "12,50 coffee #resto @cash hier"; what it understood shows underneath. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EntrySheet(state: BudgetState, viewModel: BudgetViewModel, onDismiss: () -> Unit, onDetails: (TxnInput) -> Unit) {
    val colors = Not.colors
    val input = rememberTextFieldState()
    val parsed by remember { derivedStateOf { viewModel.parse(input.text.toString()) } }
    val focus = remember { FocusRequester() }
    val highlight = remember(colors) { MoneyHighlight({ parsed }, colors) }
    val ready = parsed?.problem == null && parsed != null
    Sheet(onDismiss) { close ->
        fun submit() {
            if (ready && viewModel.quickAdd(input.text.toString())) close()
        }
        Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("New entry", style = Not.type.title, modifier = Modifier.padding(top = 4.dp))
            MoneyField(input, highlight, "12,50 coffee #resto @cash hier", ::submit, Modifier.fillMaxWidth().focusRequester(focus))
            LaunchedEffect(Unit) { focus.requestFocus() }
            val p = parsed
            if (p == null) {
                Text("Amount, then what it was. #category, @account, a day like “hier”. “/ #other 20” splits; “@sg > @cash” moves money.", style = Not.type.caption, color = colors.inkFaint)
            } else {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    p.amount?.let { Chip(R.drawable.ic_wallet, if (p.kind == TxnKind.TRANSFER) money(it) else money(it, signed = true)) }
                    if (p.kind == TxnKind.TRANSFER) {
                        Chip(R.drawable.ic_sync, "${state.account(p.account)?.name ?: "?"} → ${state.account(p.toAccount)?.name ?: "?"}")
                    } else {
                        state.account(p.account)?.let { Chip(R.drawable.ic_wallet, it.name) }
                        when {
                            p.splits.isNotEmpty() -> p.splits.forEach { s ->
                                Chip(R.drawable.ic_folder, "${state.category(s.category)?.name ?: "?"} ${money(-s.amount)}")
                            }
                            p.category != null -> Chip(R.drawable.ic_folder, (state.category(p.category)?.name ?: "?") + if (p.categoryGuessed) " (usual)" else "")
                            else -> Chip(R.drawable.ic_folder, "No category", tint = colors.inkMuted)
                        }
                    }
                    if (p.date != state.today.toString() || p.tokens.any { it.kind == MoneyTokenKind.DATE }) Chip(R.drawable.ic_calendar, dayLabel(p.date, state.today))
                }
                p.problem?.let { Text(it, style = Not.type.caption, color = colors.inkMuted) }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                Button("Details", {
                    val p2 = parsed
                    onDetails(
                        TxnInput(
                            p2?.kind ?: TxnKind.ENTRY,
                            p2?.account ?: state.accounts.first().id,
                            p2?.toAccount,
                            p2?.date ?: state.today.toString(),
                            p2?.amount ?: 0,
                            p2?.payee.orEmpty(),
                            "",
                            p2?.category,
                            p2?.splits.orEmpty(),
                        ),
                    )
                }, kind = ButtonKind.Secondary)
                Spacer(Modifier.weight(1f))
                Button("Add", ::submit, enabled = ready)
            }
        }
    }
}

@Composable
private fun MoneyField(state: TextFieldState, highlight: OutputTransformation, placeholder: String, onSubmit: () -> Unit, modifier: Modifier = Modifier) {
    val colors = Not.colors
    BasicTextField(
        state = state,
        lineLimits = TextFieldLineLimits.SingleLine,
        textStyle = Not.type.body.merge(color = colors.ink),
        cursorBrush = SolidColor(colors.accentInk),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, imeAction = ImeAction.Done),
        onKeyboardAction = { onSubmit() },
        outputTransformation = highlight,
        modifier = modifier,
        decorator = { inner ->
            Box(Modifier.clip(RoundedCornerShape(16.dp)).background(colors.sunken).padding(horizontal = 16.dp, vertical = 14.dp)) {
                if (state.text.isEmpty()) Text(placeholder, style = Not.type.body, color = colors.inkFaint, maxLines = 1)
                inner()
            }
        },
    )
}

// ---- transaction editor --------------------------------------------------------------------------

private enum class Direction(val label: String) { Out("Expense"), In("Income"), Move("Transfer") }

/** "12,50" for an amount field. */
private fun formatPlain(cents: Long): String {
    val abs = kotlin.math.abs(cents)
    return if (abs % 100 == 0L) (abs / 100).toString() else "%d,%02d".format(abs / 100, abs % 100)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun TxnSheet(state: BudgetState, viewModel: BudgetViewModel, initial: TxnInput, existing: MoneyTxn?, onDismiss: () -> Unit) {
    val colors = Not.colors
    val context = LocalContext.current
    val adjustment = initial.kind == TxnKind.ADJUSTMENT
    var direction by remember {
        mutableStateOf(
            when {
                initial.kind == TxnKind.TRANSFER -> Direction.Move
                initial.amount > 0 -> Direction.In
                initial.amount == 0L && state.category(initial.category)?.kind == CategoryKind.INCOME -> Direction.In
                else -> Direction.Out
            },
        )
    }
    var amount by remember { mutableStateOf(if (initial.amount == 0L) "" else formatPlain(initial.amount)) }
    var payee by remember { mutableStateOf(initial.payee) }
    var note by remember { mutableStateOf(initial.note) }
    var category by remember { mutableStateOf(initial.category) }
    var keepSplit by remember { mutableStateOf(initial.splits.isNotEmpty()) }
    var account by remember { mutableStateOf(initial.account) }
    var toAccount by remember { mutableStateOf(initial.toAccount ?: state.accounts.firstOrNull { it.id != initial.account }?.id) }
    var date by remember { mutableStateOf(initial.date) }
    val cents = parseCents(amount)?.let { kotlin.math.abs(it) }?.takeIf { it > 0 }

    Sheet(onDismiss) { close ->
        fun save() {
            val c = cents ?: return
            val signed = when {
                adjustment -> if (initial.amount < 0) -c else c
                direction == Direction.Out -> -c
                else -> c
            }
            val splits = if (keepSplit && direction != Direction.Move && kotlin.math.abs(signed) == kotlin.math.abs(initial.amount)) {
                initial.splits.map { it.copy(amount = if (signed < 0) -kotlin.math.abs(it.amount) else kotlin.math.abs(it.amount)) }
            } else emptyList()
            val input = TxnInput(
                kind = if (adjustment) TxnKind.ADJUSTMENT else if (direction == Direction.Move) TxnKind.TRANSFER else TxnKind.ENTRY,
                account = account,
                toAccount = if (direction == Direction.Move) toAccount else null,
                date = date,
                amount = signed,
                payee = payee,
                note = note,
                category = if (direction == Direction.Move || splits.isNotEmpty()) null else category,
                splits = splits,
            )
            val ok = if (existing == null) viewModel.add(input) else viewModel.update(existing.id, input)
            if (ok) close()
        }
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (adjustment) "Balance correction" else if (existing == null) "New entry" else "Entry", style = Not.type.title, modifier = Modifier.weight(1f).padding(top = 4.dp))
                if (existing != null) {
                    IconButton(R.drawable.ic_delete, "Delete", {
                        viewModel.delete(existing)
                        close()
                    }, tint = colors.danger)
                }
            }
            if (!adjustment) Segmented(Direction.entries.map { it.label }, direction.ordinal) { direction = Direction.entries[it] }
            AmountField(amount, { amount = it }, sign = when {
                adjustment -> if (initial.amount < 0) "−" else "+"
                direction == Direction.Out -> "−"
                direction == Direction.In -> "+"
                else -> ""
            })
            if (!adjustment) Field(payee, { payee = it }, if (direction == Direction.Move) "What for (optional)" else "Payee, e.g. Carrefour", filled = true, modifier = Modifier.fillMaxWidth())

            Label(if (direction == Direction.Move) "From" else "Account")
            ChoiceRow(state.accounts.map { it.id to it.name }, account) { account = it }
            if (direction == Direction.Move && !adjustment) {
                Label("To")
                ChoiceRow(state.accounts.filter { it.id != account }.map { it.id to it.name }, toAccount) { toAccount = it }
            }
            if (direction != Direction.Move && !adjustment) {
                Label("Category")
                if (keepSplit) {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        initial.splits.forEach { s ->
                            Text("${state.category(s.category)?.let { "${it.emoji} ${it.name}" } ?: "?"}  ${money(-s.amount)}", style = Not.type.body)
                        }
                        Text("Split. Pick a category to put it all in one.", style = Not.type.caption, color = colors.inkFaint)
                    }
                }
                val wanted = if (direction == Direction.In) CategoryKind.INCOME else CategoryKind.EXPENSE
                val shown = state.categories.sortedBy { if (it.kind == wanted) 0 else 1 }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    shown.forEach { c ->
                        FilterChip("${c.emoji} ${c.name}".trim(), !keepSplit && category == c.id) {
                            category = if (category == c.id && !keepSplit) null else c.id
                            keepSplit = false
                        }
                    }
                }
            }
            Label("Day")
            Row(
                Modifier
                    .clip(CircleShape)
                    .background(colors.sunken)
                    .pressable({
                        val d = runCatching { LocalDate.parse(date) }.getOrDefault(state.today)
                        DatePickerDialog(context, { _, y, m, day -> date = LocalDate.of(y, m + 1, day).toString() }, d.year, d.monthValue - 1, d.dayOfMonth).show()
                    }, onClickLabel = "Change day")
                    .padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(R.drawable.ic_calendar, null, tint = colors.inkMuted, size = 18.dp)
                Text(dayLabel(date, state.today, long = true), style = Not.type.label)
            }
            Field(note, { note = it }, "Note", filled = true, singleLine = false, modifier = Modifier.fillMaxWidth())
            existing?.label?.let { Text("Bank text: $it", style = Not.type.caption, color = colors.inkFaint) }
            Button(if (existing == null) "Add" else "Save", ::save, enabled = cents != null && (direction != Direction.Move || toAccount != null), modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun Label(text: String) {
    Text(text, style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(top = 2.dp))
}

@Composable
private fun ChoiceRow(options: List<Pair<String, String>>, selected: String?, onSelect: (String) -> Unit) {
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        options.forEach { (id, name) -> FilterChip(name, id == selected) { onSelect(id) } }
    }
}

/** Equal-width options in a sunken pill; the chosen one sits on a raised surface. */
@Composable
fun Segmented(options: List<String>, selected: Int, modifier: Modifier = Modifier, onSelect: (Int) -> Unit) {
    val colors = Not.colors
    Row(modifier.fillMaxWidth().clip(CircleShape).background(colors.sunken).padding(4.dp)) {
        options.forEachIndexed { i, label ->
            Box(
                Modifier
                    .weight(1f)
                    .clip(CircleShape)
                    .background(if (i == selected) colors.surface else Color.Transparent)
                    .pressable({ onSelect(i) }, pressedScale = 0.97f)
                    .heightIn(min = 38.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(label, style = Not.type.label, color = if (i == selected) colors.ink else colors.inkMuted, maxLines = 1)
            }
        }
    }
}

/** A big amount with its sign in front and a decimal keyboard. */
@Composable
private fun AmountField(value: String, onChange: (String) -> Unit, sign: String, placeholder: String = "0,00") {
    val colors = Not.colors
    val style = Not.type.display.merge(fontSize = 36.sp, lineHeight = 42.sp)
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { if (value.isEmpty()) focus.requestFocus() }
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(colors.sunken).padding(horizontal = 18.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (sign.isNotEmpty()) Text(sign, style = style, color = colors.inkMuted)
        BasicTextField(
            value,
            { onChange(it.filter { c -> c.isDigit() || c == ',' || c == '.' }) },
            singleLine = true,
            textStyle = style.merge(color = colors.ink),
            cursorBrush = SolidColor(colors.accentInk),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Next),
            modifier = Modifier.weight(1f).padding(start = 4.dp).focusRequester(focus),
            decorationBox = { inner ->
                Box {
                    if (value.isEmpty()) Text(placeholder, style = style, color = colors.inkFaint)
                    inner()
                }
            },
        )
        Text("€", style = style, color = colors.inkMuted)
    }
}

// ---- accounts ------------------------------------------------------------------------------------

private fun kindName(k: AccountKind) = when (k) {
    AccountKind.BANK -> "Bank"
    AccountKind.CASH -> "Cash"
    AccountKind.SAVINGS -> "Savings"
}

@Composable
private fun AccountsSheet(state: BudgetState, viewModel: BudgetViewModel, onDismiss: () -> Unit) {
    var editing by remember { mutableStateOf<MoneyAccount?>(null) }
    var adding by remember { mutableStateOf(false) }
    var balancing by remember { mutableStateOf<MoneyAccount?>(null) }
    val colors = Not.colors
    Sheet(onDismiss) { _ ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Row(Modifier.padding(start = 14.dp, end = 8.dp, top = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Accounts", style = Not.type.title, modifier = Modifier.weight(1f))
                Text(money(state.accounts.sumOf { it.balance }), style = Not.type.label, color = colors.inkMuted)
            }
            state.accounts.forEach { a ->
                var menu by remember { mutableStateOf(false) }
                Box {
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).pressable({ menu = true }, pressedScale = 0.99f).heightIn(min = 56.dp).padding(horizontal = 14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(a.name, style = Not.type.bodyStrong)
                            Text(kindName(a.kind), style = Not.type.caption, color = colors.inkFaint)
                        }
                        Text(money(a.balance), style = Not.type.label, color = if (a.balance < 0) colors.danger else colors.ink)
                    }
                    Menu(menu, onDismiss = { menu = false }) {
                        MenuItem("Set today's balance", icon = R.drawable.ic_edit, onClick = { menu = false; balancing = a })
                        MenuItem("Transactions", icon = R.drawable.ic_list, onClick = {
                            menu = false
                            viewModel.openTxns(TxnView(TxnFilter(a.id, null, null, null, false), a.name))
                            onDismiss()
                        })
                        MenuItem("Edit", icon = R.drawable.ic_title, onClick = { menu = false; editing = a })
                        MenuItem("Delete", icon = R.drawable.ic_delete, danger = true, onClick = { menu = false; viewModel.deleteAccount(a) })
                    }
                }
            }
            Row(
                Modifier.fillMaxWidth().clip(CircleShape).pressable({ adding = true }, pressedScale = 0.98f).heightIn(min = 52.dp).padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(R.drawable.ic_add, null, tint = colors.accentInk)
                Spacer(Modifier.width(14.dp))
                Text("New account", style = Not.type.body, color = colors.accentInk)
            }
        }
    }
    if (adding || editing != null) {
        AccountDialog(editing, onSave = { name, kind, opening ->
            editing?.let { viewModel.updateAccount(it, name, kind, opening) } ?: viewModel.createAccount(name, kind, opening)
            adding = false
            editing = null
        }, onDismiss = { adding = false; editing = null })
    }
    balancing?.let { a ->
        PromptDialog(
            title = "${a.name} today",
            confirm = "Set",
            initial = formatPlain(a.balance).let { if (a.balance < 0) "-$it" else it },
            placeholder = "What the bank says",
            onConfirm = { text ->
                parseCents(text)?.let { viewModel.setBalance(a, it) }
                balancing = null
            },
            onDismiss = { balancing = null },
        )
    }
}

@Composable
private fun AccountDialog(account: MoneyAccount?, onSave: (String, AccountKind, Long) -> Unit, onDismiss: () -> Unit) {
    var name by remember { mutableStateOf(account?.name.orEmpty()) }
    var kind by remember { mutableStateOf(account?.kind ?: AccountKind.BANK) }
    var opening by remember { mutableStateOf(account?.openingBalance?.let { (if (it < 0) "-" else "") + formatPlain(it) }.orEmpty()) }
    val openingCents = if (opening.isBlank()) 0L else parseCents(opening)
    CardDialog(
        title = if (account == null) "New account" else "Account",
        onDismiss = onDismiss,
        buttons = {
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Save", { onSave(name.trim(), kind, openingCents ?: 0) }, enabled = name.isNotBlank() && openingCents != null)
        },
    ) {
        Field(name, { name = it }, "Name, e.g. SG", filled = true, modifier = Modifier.fillMaxWidth())
        Segmented(AccountKind.entries.map(::kindName), kind.ordinal) { kind = AccountKind.entries[it] }
        Field(opening, { opening = it }, "Starting balance", filled = true, modifier = Modifier.fillMaxWidth(), trailing = { Text("€", color = Not.colors.inkMuted) })
        Text("What it held before your first entry here. Later, “Set today's balance” corrects drift.", style = Not.type.caption, color = Not.colors.inkFaint)
    }
}

// ---- categories ----------------------------------------------------------------------------------

@Composable
private fun CategoriesSheet(state: BudgetState, viewModel: BudgetViewModel, onDismiss: () -> Unit) {
    var editing by remember { mutableStateOf<MoneyCategory?>(null) }
    var adding by remember { mutableStateOf(false) }
    val colors = Not.colors
    val lines = state.budget?.let { it.expenses + it.income }.orEmpty().associateBy { it.category.id }
    Sheet(onDismiss) { _ ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Categories", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 8.dp))
            state.categories.forEach { c ->
                val line = lines[c.id]
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).pressable({ editing = c }, pressedScale = 0.99f).heightIn(min = 56.dp).padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Emoji(c.emoji)
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(c.name, style = Not.type.bodyStrong)
                        val detail = when {
                            c.kind == CategoryKind.INCOME -> "Income"
                            line != null && line.limit > 0 -> "${money(line.limit)} a period" + when (c.rollover) {
                                Rollover.LEFTOVER -> " · leftover carries"
                                Rollover.BOTH -> " · carries both ways"
                                Rollover.NONE -> ""
                            }
                            else -> "No limit"
                        }
                        Text(detail, style = Not.type.caption, color = colors.inkFaint)
                    }
                }
            }
            Row(
                Modifier.fillMaxWidth().clip(CircleShape).pressable({ adding = true }, pressedScale = 0.98f).heightIn(min = 52.dp).padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(R.drawable.ic_add, null, tint = colors.accentInk)
                Spacer(Modifier.width(14.dp))
                Text("New category", style = Not.type.body, color = colors.accentInk)
            }
        }
    }
    if (adding || editing != null) {
        val c = editing
        val oldLimit = lines[c?.id]?.limit ?: 0
        CategoryDialog(c, oldLimit, onSave = { name, emoji, kind, rollover, limit ->
            viewModel.saveCategory(c, name, emoji, kind, rollover, if (kind == CategoryKind.EXPENSE && limit != oldLimit) limit else null)
            adding = false
            editing = null
        }, onDelete = c?.let { { viewModel.deleteCategory(it); editing = null } }, onDismiss = { adding = false; editing = null })
    }
}

@Composable
private fun CategoryDialog(
    category: MoneyCategory?,
    limit: Long,
    onSave: (name: String, emoji: String, kind: CategoryKind, rollover: Rollover, limit: Long) -> Unit,
    onDelete: (() -> Unit)?,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf(category?.name.orEmpty()) }
    var emoji by remember { mutableStateOf(category?.emoji.orEmpty()) }
    var kind by remember { mutableStateOf(category?.kind ?: CategoryKind.EXPENSE) }
    var rollover by remember { mutableStateOf(category?.rollover ?: Rollover.NONE) }
    var limitText by remember { mutableStateOf(if (limit > 0) formatPlain(limit) else "") }
    val limitCents = if (limitText.isBlank()) 0L else parseCents(limitText)?.let { kotlin.math.abs(it) }
    CardDialog(
        title = if (category == null) "New category" else "Category",
        onDismiss = onDismiss,
        buttons = {
            if (onDelete != null) Button("Delete", onDelete, kind = ButtonKind.Danger)
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Save", { onSave(name.trim(), emoji.trim(), kind, rollover, limitCents ?: 0) }, enabled = name.isNotBlank() && limitCents != null)
        },
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Field(emoji, { emoji = it.take(8) }, "🙂", filled = true, modifier = Modifier.width(72.dp))
            Field(name, { name = it }, "Name", filled = true, modifier = Modifier.weight(1f))
        }
        Segmented(listOf("Expense", "Income"), kind.ordinal) { kind = CategoryKind.entries[it] }
        if (kind == CategoryKind.EXPENSE) {
            Field(limitText, { limitText = it }, "Limit per period (none)", filled = true, modifier = Modifier.fillMaxWidth(), trailing = { Text("€", color = Not.colors.inkMuted) })
            Text("From this period on; earlier periods keep theirs.", style = Not.type.caption, color = Not.colors.inkFaint)
            Text("At the end of a period", style = Not.type.label, color = Not.colors.inkMuted)
            Segmented(listOf("Start fresh", "Keep leftover", "Carry both"), rollover.ordinal) { rollover = Rollover.entries[it] }
        }
    }
}

// ---- recurring -----------------------------------------------------------------------------------

private val repeats = listOf("FREQ=MONTHLY" to "Monthly", "FREQ=WEEKLY" to "Weekly", "FREQ=YEARLY" to "Yearly")

@Composable
private fun RecurringSheet(state: BudgetState, viewModel: BudgetViewModel, onDismiss: () -> Unit) {
    var editing by remember { mutableStateOf<MoneyRecurring?>(null) }
    var adding by remember { mutableStateOf(false) }
    val colors = Not.colors
    Sheet(onDismiss) { _ ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Recurring", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp))
            Text("Rent, subscriptions, salary: each shows under Due on its day, ready to enter.", style = Not.type.caption, color = colors.inkFaint, modifier = Modifier.padding(start = 14.dp, end = 14.dp, bottom = 8.dp))
            state.recurring.forEach { r ->
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).pressable({ editing = r }, pressedScale = 0.99f).heightIn(min = 56.dp).padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Emoji(state.category(r.category)?.emoji ?: "🔁")
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(r.name, style = Not.type.bodyStrong)
                        Text("${repeats.firstOrNull { it.first == r.repeat }?.second ?: "Repeats"} · next ${dayLabel(r.nextDue, state.today)}", style = Not.type.caption, color = colors.inkFaint)
                    }
                    Text(money(r.amount, signed = true), style = Not.type.label)
                }
            }
            Row(
                Modifier.fillMaxWidth().clip(CircleShape).pressable({ adding = true }, pressedScale = 0.98f).heightIn(min = 52.dp).padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(R.drawable.ic_add, null, tint = colors.accentInk)
                Spacer(Modifier.width(14.dp))
                Text("New recurring", style = Not.type.body, color = colors.accentInk)
            }
        }
    }
    if ((adding || editing != null) && state.accounts.isNotEmpty()) {
        RecurringEditor(state, editing, onSave = { input ->
            viewModel.saveRecurring(editing?.id, input)
            adding = false
            editing = null
        }, onDelete = editing?.let { { viewModel.deleteRecurring(it); editing = null } }, onDismiss = { adding = false; editing = null })
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RecurringEditor(state: BudgetState, r: MoneyRecurring?, onSave: (RecurringInput) -> Unit, onDelete: (() -> Unit)?, onDismiss: () -> Unit) {
    val context = LocalContext.current
    var name by remember { mutableStateOf(r?.name.orEmpty()) }
    var income by remember { mutableStateOf((r?.amount ?: -1) > 0) }
    var amount by remember { mutableStateOf(r?.amount?.let(::formatPlain).orEmpty()) }
    var account by remember { mutableStateOf(r?.account ?: state.accounts.first().id) }
    var category by remember { mutableStateOf(r?.category) }
    var repeat by remember { mutableStateOf(r?.repeat ?: "FREQ=MONTHLY") }
    var next by remember { mutableStateOf(r?.nextDue ?: state.today.toString()) }
    val cents = parseCents(amount)?.let { kotlin.math.abs(it) }?.takeIf { it > 0 }
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (r == null) "New recurring" else "Recurring", style = Not.type.title, modifier = Modifier.weight(1f).padding(top = 4.dp))
                if (onDelete != null) IconButton(R.drawable.ic_delete, "Delete", { onDelete(); close() }, tint = Not.colors.danger)
            }
            Segmented(listOf("Expense", "Income"), if (income) 1 else 0) { income = it == 1 }
            AmountField(amount, { amount = it }, sign = if (income) "+" else "−")
            Field(name, { name = it }, "Name, e.g. Loyer", filled = true, modifier = Modifier.fillMaxWidth())
            Label("Repeats")
            Segmented(repeats.map { it.second }, repeats.indexOfFirst { it.first == repeat }.coerceAtLeast(0)) { repeat = repeats[it].first }
            Label("Next on")
            Row(
                Modifier.clip(CircleShape).background(Not.colors.sunken).pressable({
                    val d = runCatching { LocalDate.parse(next) }.getOrDefault(state.today)
                    DatePickerDialog(context, { _, y, m, day -> next = LocalDate.of(y, m + 1, day).toString() }, d.year, d.monthValue - 1, d.dayOfMonth).show()
                }).padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(R.drawable.ic_calendar, null, tint = Not.colors.inkMuted, size = 18.dp)
                Text(dayLabel(next, state.today, long = true), style = Not.type.label)
            }
            Label("Account")
            ChoiceRow(state.accounts.map { it.id to it.name }, account) { account = it }
            Label("Category")
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                state.categories.forEach { c -> FilterChip("${c.emoji} ${c.name}".trim(), category == c.id) { category = if (category == c.id) null else c.id } }
            }
            Button("Save", {
                val c = cents ?: return@Button
                onSave(RecurringInput(name.trim(), if (income) c else -c, account, category, repeat, next))
                close()
            }, enabled = cents != null && name.isNotBlank(), modifier = Modifier.fillMaxWidth())
        }
    }
}
