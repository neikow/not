package dev.lysen.not

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.AccountKind
import dev.lysen.not.core.CategoryKind
import dev.lysen.not.core.MoneyAccount
import dev.lysen.not.core.MoneyBudget
import dev.lysen.not.core.MoneyCategory
import dev.lysen.not.core.MoneyRecurring
import dev.lysen.not.core.MoneySettings
import dev.lysen.not.core.MoneyTxn
import dev.lysen.not.core.NotException
import dev.lysen.not.core.ParsedMoney
import dev.lysen.not.core.RecurringInput
import dev.lysen.not.core.Rollover
import dev.lysen.not.core.TxnFilter
import dev.lysen.not.core.TxnInput
import dev.lysen.not.core.formatMoney
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.LocalDate

/** Which list of transactions is open over the budget, if any. */
data class TxnView(val filter: TxnFilter, val title: String)

data class BudgetState(
    val today: LocalDate = LocalDate.now(),
    /** A day in the period on show; today's period by default. */
    val day: LocalDate = LocalDate.now(),
    val budget: MoneyBudget? = null,
    val accounts: List<MoneyAccount> = emptyList(),
    val categories: List<MoneyCategory> = emptyList(),
    val settings: MoneySettings = MoneySettings(1u, "EUR"),
    val due: List<MoneyRecurring> = emptyList(),
    val recurring: List<MoneyRecurring> = emptyList(),
    /** The transactions page, when open. */
    val txnView: TxnView? = null,
    val txns: List<MoneyTxn> = emptyList(),
    val error: String? = null,
) {
    val showsCurrentPeriod: Boolean get() = budget?.let { day.toString() in it.start..it.end && today.toString() in it.start..it.end } ?: true

    fun category(id: String?): MoneyCategory? = id?.let { c -> categories.firstOrNull { it.id == c } }
    fun account(id: String?): MoneyAccount? = id?.let { a -> accounts.firstOrNull { it.id == a } }
}

class BudgetViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    private val _state = MutableStateFlow(BudgetState())
    val state: StateFlow<BudgetState> = _state.asStateFlow()

    private val _undoables = Channel<Undoable>(Channel.CONFLATED)
    val undoables: Flow<Undoable> = _undoables.receiveAsFlow()

    init {
        run { store.seedMoney() }
        viewModelScope.launch { app.changes.collect { reload() } }
    }

    fun refreshToday() {
        val now = LocalDate.now()
        if (now != _state.value.today) {
            val following = _state.value.showsCurrentPeriod
            _state.update { it.copy(today = now, day = if (following) now else it.day) }
            reload()
        }
    }

    // ---- periods ----

    /** Steps to the previous or next period. */
    fun step(forward: Boolean) {
        val b = _state.value.budget ?: return
        val day = if (forward) LocalDate.parse(b.end).plusDays(1) else LocalDate.parse(b.start).minusDays(1)
        _state.update { it.copy(day = day) }
        reload()
    }

    fun showCurrentPeriod() {
        _state.update { it.copy(day = it.today) }
        reload()
    }

    fun setPeriodStartDay(day: Int) {
        run { store.setPeriodStartDay(day.toUInt()) }
    }

    // ---- transactions ----

    fun openTxns(view: TxnView?) {
        _state.update { it.copy(txnView = view) }
        reload()
    }

    fun parse(input: String): ParsedMoney? =
        if (input.isBlank()) null else try {
            store.parseMoney(input, today())
        } catch (_: NotException) {
            null
        }

    /** True when it went in. */
    fun quickAdd(input: String): Boolean = run { store.quickAddMoney(input, today()) } != null

    fun add(input: TxnInput): Boolean = run { store.addTxn(input) } != null

    fun update(id: String, input: TxnInput): Boolean = run { store.updateTxn(id, input) } != null

    fun setCategory(txn: MoneyTxn, category: String?) {
        run { store.setTxnCategory(txn.id, category) }
    }

    fun delete(txn: MoneyTxn) {
        run { store.deleteTxn(txn.id) } ?: return
        val what = txn.payee.ifEmpty { _state.value.category(txn.category)?.name ?: "Entry" }
        _undoables.trySend(Undoable("“$what” deleted"))
    }

    // ---- accounts ----

    fun createAccount(name: String, kind: AccountKind, opening: Long) {
        run { store.createAccount(name, kind, opening) }
    }

    fun updateAccount(account: MoneyAccount, name: String, kind: AccountKind, opening: Long) {
        run { store.updateAccount(account.id, name, kind, opening) }
    }

    fun deleteAccount(account: MoneyAccount) {
        run { store.deleteAccount(account.id) } ?: return
        _undoables.trySend(Undoable("“${account.name}” deleted"))
    }

    fun setBalance(account: MoneyAccount, balance: Long) {
        val adjusted = run { store.setBalance(account.id, balance, today()) } ?: return
        if (adjusted != null) _undoables.trySend(Undoable("${account.name} set to ${formatMoney(balance, false)}"))
    }

    // ---- categories ----

    /** Creates or updates; [limit] (null = unchanged) applies from the period on show onwards. */
    fun saveCategory(existing: MoneyCategory?, name: String, emoji: String, kind: CategoryKind, rollover: Rollover, limit: Long?) {
        val id = existing?.id ?: run { store.createCategory(name, emoji, kind) } ?: return
        run { store.updateCategory(id, name, emoji, kind, rollover) }
        if (limit != null) run { store.setCategoryLimit(id, _state.value.day.toString(), limit) }
    }

    fun deleteCategory(category: MoneyCategory) {
        run { store.deleteCategory(category.id) } ?: return
        _undoables.trySend(Undoable("“${category.name}” deleted"))
    }

    fun moveCategory(id: String, index: Int) {
        run { store.moveCategory(id, index.toUInt()) }
    }

    // ---- recurring ----

    fun saveRecurring(id: String?, input: RecurringInput) {
        run { if (id == null) store.createRecurring(input) else store.updateRecurring(id, input) }
    }

    fun deleteRecurring(r: MoneyRecurring) {
        run { store.deleteRecurring(r.id) } ?: return
        _undoables.trySend(Undoable("“${r.name}” deleted"))
    }

    fun enterRecurring(r: MoneyRecurring, amount: Long? = null) {
        run { store.enterRecurring(r.id, amount) } ?: return
        _undoables.trySend(Undoable("“${r.name}” entered"))
    }

    fun skipRecurring(r: MoneyRecurring) {
        run { store.skipRecurring(r.id) } ?: return
        _undoables.trySend(Undoable("“${r.name}” skipped"))
    }

    fun undo() {
        run { store.undo() }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    /** Debug builds: a few accounts and a month of spending on an empty budget, for screenshots. */
    fun seedDemo() {
        if (!BuildConfig.DEBUG || _state.value.accounts.isNotEmpty()) return
        val sg = run { store.createAccount("SG", AccountKind.BANK, 182_340) } ?: return
        val bourso = run { store.createAccount("Boursobank", AccountKind.BANK, 640_000) } ?: return
        run { store.createAccount("Cash", AccountKind.CASH, 4_000) }
        val cats = store.moneyCategories().associateBy { it.name }
        fun limit(name: String, cents: Long) = cats[name]?.let { run { store.setCategoryLimit(it.id, today(), cents) } }
        limit("Courses", 45_000)
        limit("Resto", 15_000)
        limit("Transport", 8_000)
        limit("Loisirs", 10_000)
        limit("Shopping", 12_000)
        val start = LocalDate.parse(store.budget(today()).start)
        fun on(daysIn: Long) = start.plusDays(daysIn).coerceAtMost(_state.value.today)
        listOf(
            "+2450 Salaire #revenus @sg" to 0L, "62,40 Carrefour #courses @sg" to 1L, "4,20 Boulangerie #courses @cash" to 2L,
            "38 Le Petit Zinc #resto @bourso" to 3L, "86,10 Navigo #transport @sg" to 3L, "17,99 Spotify #abonnements @bourso" to 4L,
            "54,75 Monoprix #courses @sg" to 6L, "24 Cinéma #loisirs @bourso" to 7L, "120 Fnac #shopping 90 / #loisirs @sg" to 8L,
            "12,50 Pharmacie #sante @cash" to 9L, "41,80 Franprix #courses @sg" to 10L, "500 @sg > @bourso" to 10L,
            "29 Sushi Shop #resto @sg" to 11L, "15 Uber" to 12L,
        ).forEach { (text, d) -> run { store.quickAddMoney(text, on(d).toString()) } }
        run {
            store.createRecurring(RecurringInput("Loyer", -95_000, sg, cats["Logement"]?.id, "FREQ=MONTHLY", on(4).toString()))
        }
        run {
            store.createRecurring(RecurringInput("Netflix", -1_399, bourso, cats["Abonnements"]?.id, "FREQ=MONTHLY", start.plusDays(20).toString()))
        }
    }

    private fun today() = _state.value.today.toString()

    private fun reload() {
        val s = _state.value
        val budget = run { store.budget(s.day.toString()) } ?: return
        val accounts = run { store.accounts(s.today.toString()) }.orEmpty()
        val txns = s.txnView?.let { run { store.transactions(it.filter) } }.orEmpty()
        _state.update {
            it.copy(
                budget = budget,
                accounts = accounts,
                categories = store.moneyCategories(),
                settings = store.moneySettings(),
                due = run { store.dueRecurring(s.today.toString()) }.orEmpty(),
                recurring = store.recurring(),
                txns = txns,
            )
        }
    }

    private fun <T> run(body: () -> T): T? =
        try {
            body()
        } catch (e: NotException) {
            _state.update { it.copy(error = e.userMessage) }
            null
        }
}
