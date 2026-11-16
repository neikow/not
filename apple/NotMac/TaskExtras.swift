import NotCore
import SwiftUI

/// A smart list being made (no `id`) or edited.
struct ViewDraft: Identifiable {
    var id: String?
    var title = ""
    var query = ""
}

/// "Tomorrow 9:00 ⟳ · ⏳ Fri · 2/5 · #home" under a task's title.
struct TaskMeta: View {
    let task: TaskItem
    let today: String
    var done = false
    var hiddenTag: String?
    var subtasks: (UInt32, UInt32) = (0, 0)

    var body: some View {
        let tags = task.tags.filter { $0 != hiddenTag }
        let (total, finished) = subtasks
        if task.start != nil || task.deadline != nil || task.someday || !tags.isEmpty || total > 0 {
            HStack(spacing: 4) {
                if let start = task.start {
                    Text(scheduleLabel(due: start, time: task.time, repeat: nil, today: today) ?? start)
                        .foregroundStyle(done ? AnyShapeStyle(.tertiary) : start == today ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                    if task.repeat != nil { Image(systemName: "repeat").imageScale(.small).foregroundStyle(.secondary) }
                }
                if task.someday { Text("Someday").foregroundStyle(.tertiary) }
                if let deadline = task.deadline {
                    let style = done ? AnyShapeStyle(.tertiary) : deadline < today ? AnyShapeStyle(.red) : deadline == today ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary)
                    Image(systemName: "hourglass").imageScale(.small).foregroundStyle(style)
                    Text(scheduleLabel(due: deadline, time: nil, repeat: nil, today: today) ?? deadline).foregroundStyle(style)
                }
                if total > 0 { Text("\(finished)/\(total)").foregroundStyle(.tertiary) }
                if !tags.isEmpty { Text(tags.map { "#" + $0 }.joined(separator: " ")).foregroundStyle(.tertiary) }
            }
            .font(.caption)
        }
    }
}

/// Makes or edits a smart list: a name and a filter, checked as it's typed.
struct ViewEditor: View {
    let model: AppModel
    let draft: ViewDraft
    @State private var title = ""
    @State private var query = ""
    @Environment(\.dismiss) private var dismiss

    private var problem: String? { query.isEmpty ? nil : model.checkQuery(query) }
    private var ready: Bool { !title.trimmingCharacters(in: .whitespaces).isEmpty && !query.isEmpty && problem == nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(draft.id == nil ? "New Smart List" : "Edit Smart List").font(.headline)
            TextField("Name", text: $title, prompt: Text("Work, Errands, This week…"))
            TextField("Filter", text: $query, prompt: Text("#work !1, #home or #errands, week…"))
                .font(.body.monospaced())
                .onSubmit(save)
            Text(problem ?? (query.isEmpty ? "Words match together; “or” joins alternatives; “-” leaves out." : "\(model.countQuery(query)) tasks now"))
                .font(.caption)
                .foregroundStyle(problem == nil ? AnyShapeStyle(.secondary) : AnyShapeStyle(.red))
            ScrollView(.horizontal, showsIndicators: false) {
                HStack {
                    ForEach(model.tags.prefix(10), id: \.name) { tag in
                        Button("#" + tag.name) { add("#" + tag.name) }
                    }
                    ForEach([("Today", "today"), ("This week", "week"), ("Overdue", "overdue"), ("Deadline", "deadline"),
                             ("High", "!1"), ("No date", "nodate"), ("Someday", "someday"), ("Untagged", "untagged"), ("or", "or")], id: \.1) { label, term in
                        Button(label) { add(term) }
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(draft.id == nil ? "Create" : "Save", action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ready)
            }
        }
        .padding(20)
        .frame(width: 460)
        .onAppear {
            title = draft.title
            query = draft.query
        }
    }

    private func add(_ term: String) {
        query = (query.trimmingCharacters(in: .whitespaces) + " " + term).trimmingCharacters(in: .whitespaces)
    }

    private func save() {
        guard ready else { return }
        if let id = draft.id { model.updateList(id, title: title, query: query) } else { model.addList(title, query: query) }
        dismiss()
    }
}

/// The weekly review: Inbox, what slipped, Someday, and tasks untouched for a month.
struct ReviewSheet: View {
    let model: AppModel
    @State private var step = 0
    @State private var handled: Set<String> = []
    @State private var tasks: [TaskItem] = []
    @Environment(\.dismiss) private var dismiss

    private let steps = [
        ("Inbox", "Give each a day, a tag or Someday — or let it go."),
        ("Slipped", "Planned for an earlier day and not done yet."),
        ("Someday", "Still maybe? Bring one back or let it go."),
        ("Untouched for a month", "No date and no change in 30 days: still relevant?"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Weekly Review").font(.headline)
                Spacer()
                Text(step < steps.count ? "Step \(step + 1) of \(steps.count)" : "Done").foregroundStyle(.secondary)
            }
            if step < steps.count {
                Text(steps[step].0).font(.title3.weight(.semibold))
                Text(steps[step].1).foregroundStyle(.secondary)
                let shown = tasks.filter { !handled.contains($0.id) }
                if shown.isEmpty {
                    Text("Nothing here.").foregroundStyle(.tertiary).frame(maxWidth: .infinity, minHeight: 80)
                } else {
                    if step == 1 {
                        Button("Move All to Today") {
                            model.moveToToday(shown.map(\.id))
                            handled.formUnion(shown.map(\.id))
                        }
                    }
                    List(shown, id: \.id) { task in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(task.title)
                                TaskMeta(task: task, today: model.today)
                            }
                            Spacer()
                            actions(for: task)
                        }
                    }
                    .frame(minHeight: 240)
                }
            } else {
                ContentUnavailableView("Reviewed", systemImage: "checkmark.seal", description: Text("Everything has a place. See you next week."))
            }
            HStack {
                Spacer()
                if step > 0, step < steps.count { Button("Back") { go(step - 1) } }
                if step < steps.count {
                    Button("Next") { go(step + 1) }.keyboardShortcut(.defaultAction)
                } else {
                    Button("Close") { dismiss() }.keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(width: 560)
        .onAppear { go(0) }
    }

    private func go(_ to: Int) {
        step = to
        switch to {
        case 0: tasks = model.viewTasks(model.inboxID)
        case 1: tasks = model.viewTasks(model.todayID).filter { $0.start.map { $0 < model.today } ?? false }
        case 2: tasks = model.viewTasks(model.somedayID)
        case 3: tasks = model.staleTasks()
        default: tasks = []
        }
    }

    @ViewBuilder private func actions(for task: TaskItem) -> some View {
        HStack(spacing: 4) {
            Button("Today") { model.moveToToday([task.id]); handled.insert(task.id) }
            if step == 0 || step == 1 {
                Button("Next Week") { model.snooze([task.id], .nextWeek); handled.insert(task.id) }
            }
            if step == 2 || step == 3 {
                Button("Keep") { model.keep(task.id); handled.insert(task.id) }
            }
            if step != 2 {
                Button("Someday") { model.setSomeday(task); handled.insert(task.id) }
            }
            Button("Done") { model.toggle(task); handled.insert(task.id) }
            Button("Delete", role: .destructive) { model.delete(id: task.id); handled.insert(task.id) }
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }
}

/// Everything completed, by day, latest first.
struct LogbookSheet: View {
    let model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let done = model.logbook()
        let days = Dictionary(grouping: done) { task in
            Calendar.current.startOfDay(for: Date(timeIntervalSince1970: Double(task.completedAt ?? 0) / 1000))
        }
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Logbook").font(.headline)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
            if done.isEmpty {
                ContentUnavailableView("Nothing yet", systemImage: "book.closed", description: Text("Tasks you complete are kept here, by day."))
            } else {
                List {
                    ForEach(days.keys.sorted(by: >), id: \.self) { day in
                        Section(Self.label(day)) {
                            ForEach(days[day] ?? [], id: \.id) { task in
                                HStack {
                                    Button { model.toggle(task) } label: { Image(systemName: "checkmark.circle.fill") }
                                        .buttonStyle(.plain)
                                        .foregroundStyle(.tint)
                                        .help("Reopen")
                                    Text(task.title).strikethrough().foregroundStyle(.secondary)
                                    Spacer()
                                    if !task.tags.isEmpty {
                                        Text(task.tags.map { "#" + $0 }.joined(separator: " ")).font(.caption).foregroundStyle(.tertiary)
                                    }
                                }
                            }
                        }
                    }
                }
                .frame(minHeight: 360)
            }
        }
        .padding(20)
        .frame(width: 480)
    }

    private static func label(_ day: Date) -> String {
        if Calendar.current.isDateInToday(day) { return "Today" }
        if Calendar.current.isDateInYesterday(day) { return "Yesterday" }
        return day.formatted(date: .abbreviated, time: .omitted)
    }
}
