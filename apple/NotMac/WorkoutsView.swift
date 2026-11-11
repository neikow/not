import Charts
import NotCore
import SwiftUI

/// Sessions, latest first, beside the selected session's exercises.
struct WorkoutsView: View {
    @Bindable var model: AppModel
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        HSplitView {
            WorkoutList(model: model)
                .frame(minWidth: 220, idealWidth: 260, maxWidth: 360)
            Group {
                if let workout = model.workout {
                    WorkoutEditor(model: model, workout: workout)
                        .id(workout.id)
                } else {
                    ContentUnavailableView {
                        Label(model.workouts.isEmpty ? "No Workouts Yet" : "No Workout Selected", systemImage: "dumbbell")
                    } description: {
                        if model.workouts.isEmpty {
                            Text("Start one when you hit the gym, or bring in what you've logged elsewhere.")
                        }
                    } actions: {
                        Button("Start Workout") { model.newWorkout() }
                        if model.workouts.isEmpty {
                            // Importing lives in Settings → Workouts.
                            Button("Import from Text…") { openSettings() }
                        }
                    }
                }
            }
            .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle("Workouts")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                RestTimerButton(timer: model.restTimer)
                Menu {
                    Button("Empty Workout") { model.newWorkout() }
                    let recent = model.workouts.filter { !$0.exerciseNames.isEmpty }.prefix(8)
                    if !recent.isEmpty {
                        Section("Repeat") {
                            ForEach(recent, id: \.id) { w in
                                Button(workoutName(w)) { model.repeatWorkout(w.id) }
                            }
                        }
                    }
                } label: {
                    Label("New Workout", systemImage: "plus")
                } primaryAction: {
                    model.newWorkout()
                }
                .help("New workout (⌘N)")
            }
        }
    }
}

// MARK: - List

private struct WorkoutList: View {
    @Bindable var model: AppModel

    var body: some View {
        List(selection: $model.selectedWorkoutID) {
            if !model.weekMuscles.isEmpty {
                Section("Sets this week") {
                    Text(model.weekMuscles.prefix(8).map { "\($0.muscle) \($0.sets)" }.joined(separator: " · "))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .selectionDisabled()
                }
            }
            ForEach(model.workouts, id: \.id) { w in
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(w.title.isEmpty ? dayLabel(w.date, today: model.today) : w.title)
                            .font(.headline)
                        Spacer()
                        if !w.title.isEmpty {
                            Text(dayLabel(w.date, today: model.today)).foregroundStyle(.secondary)
                        }
                    }
                    Text(w.exerciseNames.isEmpty ? "No exercises" : w.exerciseNames.joined(separator: " · "))
                        .font(.callout)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
                }
                .padding(.vertical, 4)
                .tag(w.id)
            }
        }
        .listStyle(.inset)
        .animation(.snappy, value: model.workouts.map(\.id))
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Repeat Today") { model.repeatWorkout(id) }
                Divider()
                Button("Delete", role: .destructive) { model.deleteWorkout(id) }
            }
        }
        .onDeleteCommand { if let id = model.selectedWorkoutID { model.deleteWorkout(id) } }
    }
}

// MARK: - Editor

/// Where typing goes in the editor.
private enum Field: Hashable {
    case title
    case sets(String)
    case newExercise
}

private struct WorkoutEditor: View {
    let model: AppModel
    let workout: Workout
    @State private var title = ""
    @State private var newExercise = ""
    @FocusState private var focus: Field?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                header
                if !workout.exercises.isEmpty { SessionProgress(exercises: workout.exercises) }
                ForEach(Array(workout.exercises.enumerated()), id: \.element.id) { index, exercise in
                    ExerciseCard(
                        model: model,
                        exercise: exercise,
                        index: index,
                        count: workout.exercises.count,
                        focus: $focus
                    )
                }
                addExercise
            }
            .padding(.horizontal, 28)
            .padding(.vertical, 22)
            .frame(maxWidth: 760, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .onAppear {
            title = workout.title
            if workout.exercises.isEmpty { focus = .newExercise }
        }
        .onChange(of: workout.title) { _, synced in if focus != .title { title = synced } }
        .onChange(of: focus) { old, _ in if old == .title { saveTitle() } }
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            TextField("Workout", text: $title, prompt: Text(dayLabel(workout.date, today: model.today)))
                .textFieldStyle(.plain)
                .font(.largeTitle.bold())
                .focused($focus, equals: .title)
                .onSubmit(saveTitle)
            DayButton(day: workout.date, today: model.today) { day in
                model.updateWorkout(workout.id, date: day, title: workout.title)
            }
        }
        .padding(.bottom, 6)
    }

    private var addExercise: some View {
        HStack(spacing: 10) {
            Image(systemName: "plus.circle.fill")
                .foregroundStyle(.tint)
                .imageScale(.large)
            TextField("Add exercise", text: $newExercise)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($focus, equals: .newExercise)
                .textInputSuggestions {
                    let taken = Set(workout.exercises.map { $0.name.lowercased() })
                    let mine = suggestions.filter { !taken.contains($0.lowercased()) }.prefix(6)
                    ForEach(Array(mine), id: \.self) { name in
                        Label(name, systemImage: "clock.arrow.circlepath").textInputCompletion(name)
                    }
                    let typed = newExercise.trimmingCharacters(in: .whitespaces)
                    if typed.count >= 2 {
                        let seen = Set(mine.map { $0.lowercased() }).union(taken)
                        ForEach(model.searchExercises(typed).filter { !seen.contains(catalogName($0, typed).lowercased()) }, id: \.id) { e in
                            let name = catalogName(e, typed)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(name)
                                Text(([otherName(e, name)].compactMap { $0 } + e.muscles.prefix(2)).joined(separator: " · "))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .textInputCompletion(name)
                        }
                    }
                }
                .onSubmit {
                    guard let id = model.addExercise(newExercise, to: workout.id) else { return }
                    newExercise = ""
                    focus = .sets(id)
                }
        }
        .padding(.top, 4)
    }

    /// Names used before, matching what's typed so far.
    private var suggestions: [String] {
        let typed = newExercise.trimmingCharacters(in: .whitespaces)
        guard !typed.isEmpty else { return model.exerciseNames }
        return model.exerciseNames.filter { $0.localizedCaseInsensitiveContains(typed) }
    }

    private func saveTitle() {
        if title != workout.title { model.updateWorkout(workout.id, date: workout.date, title: title) }
    }
}

private struct ExerciseCard: View {
    let model: AppModel
    let exercise: WorkoutExercise
    let index: Int
    let count: Int
    var focus: FocusState<Field?>.Binding
    @State private var name = ""
    @State private var draft = ""
    @State private var editingLine: Int?
    @State private var lineDraft = ""
    @State private var showsHistory = false
    @FocusState private var nameFocused: Bool
    @FocusState private var lineFocused: Bool

    private var lines: [String] { exercise.lines.map(\.raw) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Button { model.setExerciseDone(exercise.id, !exercise.done) } label: {
                    Image(systemName: exercise.done ? "checkmark.circle.fill" : "circle")
                        .imageScale(.large)
                        .foregroundStyle(exercise.done ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                }
                .buttonStyle(.borderless)
                .help(exercise.done ? "Mark not done" : "Mark done")
                TextField("Exercise", text: $name)
                    .textFieldStyle(.plain)
                    .font(.headline)
                    .focused($nameFocused)
                    .onSubmit(saveName)
                    .onChange(of: nameFocused) { _, focused in if !focused { saveName() } }
                Spacer(minLength: 0)
                Button { showsHistory = true } label: {
                    Image(systemName: "chart.line.uptrend.xyaxis")
                }
                .buttonStyle(.borderless)
                .help("History")
                .popover(isPresented: $showsHistory, arrowEdge: .bottom) {
                    ExerciseHistory(model: model, name: exercise.name, logs: model.exerciseHistory(exercise.name), today: model.today)
                }
                Menu {
                    Button("Move Up") { model.moveExercise(exercise.id, to: index - 1) }
                        .disabled(index == 0)
                    Button("Move Down") { model.moveExercise(exercise.id, to: index + 1) }
                        .disabled(index == count - 1)
                    Divider()
                    Button("Delete Exercise", role: .destructive) { model.deleteExercise(exercise.id) }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
            }

            if let info = model.exerciseInfo(exercise.name) {
                Text((info.muscles + info.equipment).joined(separator: " · "))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .padding(.top, -6)
            }

            if let previous = exercise.previous, !exercise.done {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text("Last · \(dayLabel(previous.date, today: model.today))")
                        .foregroundStyle(.secondary)
                    Text(previous.lines.map(\.label).joined(separator: "  ·  "))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                    if exercise.sets.isEmpty {
                        Button("Same Again") { model.updateExercise(exercise.id, name: exercise.name, sets: previous.sets) }
                            .buttonStyle(.borderless)
                            .foregroundStyle(.tint)
                            .help("Copy last time's sets")
                    }
                }
                .font(.callout)
            }

            ForEach(Array(exercise.lines.enumerated()), id: \.offset) { i, line in
                if editingLine == i {
                    TextField("Sets", text: $lineDraft)
                        .textFieldStyle(.roundedBorder)
                        .focused($lineFocused)
                        .onAppear { lineFocused = true }
                        .onSubmit { commitLine(i) }
                        .onExitCommand { editingLine = nil }
                        .onChange(of: lineFocused) { _, focused in if !focused { commitLine(i) } }
                } else {
                    SetLineRow(line: line)
                        .contentShape(.rect)
                        .onTapGesture(count: 2) {
                            lineDraft = line.raw
                            editingLine = i
                        }
                        .contextMenu {
                            Button("Edit") {
                                lineDraft = line.raw
                                editingLine = i
                            }
                            Button("Delete", role: .destructive) { replaceLine(i, with: nil) }
                        }
                }
            }

            if !exercise.done { addSets }
        }
        .padding(14)
        // Ticked-off exercises step back so what's left stands out.
        .opacity(exercise.done ? 0.5 : 1)
        .animation(.easeOut(duration: 0.2), value: exercise.done)
        .background(.background.secondary, in: .rect(cornerRadius: 12))
        .onAppear { name = exercise.name }
        .onChange(of: exercise.name) { _, synced in if !nameFocused { name = synced } }
    }

    private var addSets: some View {
        let preview = draft.trimmingCharacters(in: .whitespaces).isEmpty ? nil : parseSets(text: draft).first
        return HStack(spacing: 8) {
            Image(systemName: "plus")
                .foregroundStyle(.secondary)
                .imageScale(.small)
            TextField("Sets", text: $draft, prompt: Text(exercise.previous?.lines.last { $0.parsed }?.raw ?? "24kg x 10 x 3"))
                .textFieldStyle(.plain)
                .focused(focus, equals: .sets(exercise.id))
                .onSubmit(addLine)
            if let preview {
                Text(preview.parsed ? preview.label : "Note")
                    .font(.callout)
                    .foregroundStyle(preview.parsed ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                    .help(preview.parsed ? "Read as sets" : "Not read as sets; kept as a note")
            }
        }
        .padding(.top, exercise.lines.isEmpty ? 0 : 2)
    }

    private func addLine() {
        let text = draft.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return }
        let parsed = parseSets(text: text).first
        model.updateExercise(exercise.id, name: exercise.name, sets: (lines + [text]).joined(separator: "\n"))
        draft = ""
        // Sets only: a swim or a run isn't followed by a rest.
        if parsed?.parsed == true, parsed?.warmup == false, parsed?.reps.isEmpty == false, model.restTimer.startsAfterSet {
            model.restTimer.start()
        }
    }

    private func commitLine(_ i: Int) {
        guard editingLine == i else { return }
        editingLine = nil
        let text = lineDraft.trimmingCharacters(in: .whitespaces)
        if text != lines[i] { replaceLine(i, with: text.isEmpty ? nil : text) }
    }

    private func replaceLine(_ i: Int, with text: String?) {
        var all = lines
        guard all.indices.contains(i) else { return }
        if let text { all[i] = text } else { all.remove(at: i) }
        model.updateExercise(exercise.id, name: exercise.name, sets: all.joined(separator: "\n"))
    }

    private func saveName() {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { name = exercise.name; return }
        if trimmed != exercise.name { model.updateExercise(exercise.id, name: trimmed, sets: exercise.sets) }
    }
}

/// How far through the session: a thin bar with "2 of 5" beside it.
private struct SessionProgress: View {
    let exercises: [WorkoutExercise]

    var body: some View {
        let done = exercises.filter(\.done).count
        HStack(spacing: 12) {
            ProgressView(value: Double(done), total: Double(exercises.count))
                .progressViewStyle(.linear)
                .animation(.spring(duration: 0.4), value: done)
            Text(done == exercises.count ? "All done" : "\(done) of \(exercises.count)")
                .font(.caption)
                .foregroundStyle(done == exercises.count ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                .monospacedDigit()
        }
    }
}

private struct SetLineRow: View {
    let line: SetLine

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(line.parsed ? (line.warmup ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.tint)) : AnyShapeStyle(.clear))
                .frame(width: 6, height: 6)
            Text(line.label)
                .foregroundStyle(line.parsed && !line.warmup ? .primary : .secondary)
                .italic(!line.parsed)
            if let note = line.note {
                Text(note)
                    .font(.body)
                    .italic()
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .font(.body.monospacedDigit())
    }
}

/// "Today" / "Mon 2 Nov" as a quiet button; opens a calendar to move the session to another day.
private struct DayButton: View {
    let day: String
    let today: String
    let onPick: (String) -> Void
    @State private var picking = false

    var body: some View {
        Button { picking = true } label: {
            // The actual date: the title above already says "Today" or "Yesterday".
            Label(parseDay(day)?.formatted(.dateTime.weekday(.wide).day().month(.wide)) ?? day, systemImage: "calendar")
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(.quaternary.opacity(0.6), in: .capsule)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Change the date")
        .popover(isPresented: $picking, arrowEdge: .bottom) {
            VStack(spacing: 8) {
                DatePicker("Date", selection: Binding(
                    get: { parseDay(day) ?? .now },
                    set: {
                        onPick(AppModel.dayString($0))
                        picking = false
                    }
                ), in: ...Date.now, displayedComponents: .date)
                .datePickerStyle(.graphical)
                .labelsHidden()
                if day != today {
                    Button("Today") {
                        onPick(today)
                        picking = false
                    }
                    .buttonStyle(.borderless)
                    .foregroundStyle(.tint)
                }
            }
            .padding(12)
        }
    }
}

// MARK: - History

private struct ExerciseHistory: View {
    let model: AppModel
    let name: String
    let logs: [ExerciseLog]
    let today: String
    @State private var linking = false
    @State private var metric = Metric.top
    @State private var picked: Date?

    /// What the chart plots, one at a time (never two scales on one axis).
    enum Metric: String, CaseIterable {
        case top = "Top set", oneRM = "Est. 1RM", volume = "Volume"
    }

    /// Cardio (no weights): distance in km instead of the top set.
    private var isCardio: Bool { logs.allSatisfy { $0.topWeight == nil } && logs.contains { $0.distance > 0 } }

    private func value(_ log: ExerciseLog) -> Double? {
        if isCardio { return log.distance > 0 ? log.distance / 1000 : nil }
        switch metric {
        case .top: return log.topWeight
        // Epley over working sets: weight × (1 + reps / 30), best of the session.
        case .oneRM:
            return log.lines.filter { $0.parsed && !$0.warmup }
                .flatMap { line in line.reps.compactMap { reps in line.weight.map { $0 * (1 + Double(reps) / 30) } } }
                .max()
        case .volume: return log.volume > 0 ? log.volume : nil
        }
    }

    private var points: [(date: Date, weight: Double)] {
        logs.reversed().compactMap { log in
            guard let d = parseDay(log.date), let w = value(log) else { return nil }
            return (d, w)
        }
    }

    private var unit: String { isCardio ? "km" : "kg" }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(name).font(.headline)
            CatalogLink(model: model, name: name, linking: $linking)
            if linking {
                EmptyView()
            } else if logs.isEmpty {
                Text("Nothing logged yet.").foregroundStyle(.secondary)
            } else {
                if !isCardio {
                    Picker("Chart", selection: $metric) {
                        ForEach(Metric.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
                if points.count > 1 {
                    let shown = picked.flatMap { day in points.min { abs($0.date.timeIntervalSince(day)) < abs($1.date.timeIntervalSince(day)) } } ?? points.last!
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text("\(formatWeight((shown.weight * 10).rounded() / 10)) \(unit)").font(.title3.weight(.semibold))
                        Text(picked == nil ? "latest" : shown.date.formatted(.dateTime.day().month()))
                            .foregroundStyle(.secondary)
                    }
                    Chart {
                        ForEach(points, id: \.date) { p in
                            LineMark(x: .value("Date", p.date), y: .value(metric.rawValue, p.weight))
                                .lineStyle(StrokeStyle(lineWidth: 2))
                            PointMark(x: .value("Date", p.date), y: .value(metric.rawValue, p.weight))
                                .symbolSize(p.date == shown.date && picked != nil ? 80 : 36)
                        }
                        if picked != nil {
                            RuleMark(x: .value("Date", shown.date))
                                .foregroundStyle(.secondary)
                                .lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                        }
                    }
                    .chartYScale(domain: .automatic(includesZero: false))
                    .chartXSelection(value: $picked)
                    .frame(height: 140)
                } else if !logs.isEmpty {
                    Text("Log this exercise twice to see a trend.").font(.callout).foregroundStyle(.secondary)
                }
                if isCardio, let longest = logs.map(\.distance).max() {
                    Text("Longest \(formatWeight(longest / 1000)) km · \(logs.count) sessions")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                } else if let best = logs.compactMap(\.topWeight).max() {
                    Text("Best \(formatWeight(best)) kg · \(logs.count) sessions")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(logs, id: \.workoutId) { log in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(dayLabel(log.date, today: today)).font(.callout.weight(.semibold))
                                ForEach(Array(log.lines.enumerated()), id: \.offset) { _, line in
                                    Text(line.note.map { "\(line.label) — \($0)" } ?? line.label)
                                        .font(.callout.monospacedDigit())
                                        .foregroundStyle(line.warmup || !line.parsed ? .secondary : .primary)
                                }
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 260)
            }
        }
        .padding(16)
        .frame(width: 320)
    }
}

/// What the name is linked to in wger's database, with its muscles and credit; or a search to
/// link it.
private struct CatalogLink: View {
    let model: AppModel
    let name: String
    @Binding var linking: Bool
    @State private var query = ""

    var body: some View {
        let info = model.exerciseInfo(name)
        if linking {
            VStack(alignment: .leading, spacing: 6) {
                TextField("Search exercises", text: $query)
                    .textFieldStyle(.roundedBorder)
                    .onAppear { query = info?.nameEn ?? name.replacing(/\(.*?\)/, with: "") }
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(model.searchExercises(query, limit: 20), id: \.id) { e in
                            Button {
                                model.linkExercise(name, to: e.id)
                                linking = false
                            } label: {
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(e.nameFr ?? e.nameEn)
                                    Text(([e.nameFr == nil ? nil : e.nameEn].compactMap { $0 } + e.muscles).joined(separator: " · "))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.vertical, 4)
                                .contentShape(.rect)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .frame(height: 220)
                HStack {
                    if info != nil {
                        Button("Unlink", role: .destructive) {
                            model.linkExercise(name, to: nil)
                            linking = false
                        }
                    }
                    Spacer()
                    Button("Cancel") { linking = false }
                }
                .buttonStyle(.borderless)
            }
        } else if let info {
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline) {
                    Text(info.nameFr.map { $0 == name ? info.nameEn : $0 } ?? info.nameEn)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Change") { linking = true }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.tint)
                }
                Text(muscleLine(info))
                    .font(.callout)
                Text("wger.de\(info.author.isEmpty ? "" : " · \(info.author)") · \(info.license)")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        } else {
            Button("Link to an Exercise…") { linking = true }
                .buttonStyle(.borderless)
                .foregroundStyle(.tint)
                .help("Link to wger's exercise database for muscles, and to share history with other names for it")
        }
    }

    private func muscleLine(_ e: CatalogExercise) -> String {
        var parts = [e.muscles.joined(separator: ", ")]
        if !e.secondaryMuscles.isEmpty { parts.append("also " + e.secondaryMuscles.joined(separator: ", ")) }
        if !e.equipment.isEmpty { parts.append(e.equipment.joined(separator: ", ")) }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// The catalogue name to insert: the one that matches what was typed, else French, else English.
func catalogName(_ e: CatalogExercise, _ typed: String) -> String {
    let candidates = [e.nameFr, e.nameEn].compactMap { $0 }
    return candidates.first { $0.localizedStandardContains(typed) } ?? e.nameFr ?? e.nameEn
}

/// The exercise's name in the other language, if it has one.
func otherName(_ e: CatalogExercise, _ shown: String) -> String? {
    let other = shown == e.nameEn ? e.nameFr : e.nameEn
    return other == shown ? nil : other
}

// MARK: - Rest timer

private struct RestTimerButton: View {
    let timer: RestTimer

    var body: some View {
        Menu {
            Section("Rest For") {
                ForEach(RestTimer.presets, id: \.self) { s in
                    Button(RestTimer.format(s)) { timer.start(seconds: s) }
                }
            }
            if timer.isRunning {
                Button("Add 15 Seconds") { timer.extend(by: 15) }
                Button("Take Off 15 Seconds") { timer.extend(by: -15) }
                Button("Stop") { timer.stop() }
            }
            Divider()
            Toggle("Start After Each Set", isOn: Binding(get: { timer.startsAfterSet }, set: { timer.startsAfterSet = $0 }))
        } label: {
            if let ends = timer.endsAt {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Label(RestTimer.format(max(0, Int(ends.timeIntervalSince(context.date).rounded(.up)))), systemImage: "timer")
                        .labelStyle(.titleAndIcon)
                        .monospacedDigit()
                }
            } else {
                Label("Rest \(RestTimer.format(timer.duration))", systemImage: "timer")
            }
        } primaryAction: {
            timer.isRunning ? timer.stop() : timer.start()
        }
        .help(timer.isRunning ? "Stop the rest timer" : "Start a \(RestTimer.format(timer.duration)) rest")
    }
}

// MARK: - Formatting

func workoutName(_ w: WorkoutSummary) -> String {
    let day = parseDay(w.date)?.formatted(.dateTime.day().month(.abbreviated)) ?? w.date
    let what = w.title.isEmpty ? w.exerciseNames.prefix(2).joined(separator: ", ") : w.title
    return "\(what) · \(day)"
}

/// "Today", "Yesterday", "Mon 2 Nov", or with the year when it isn't this one.
func dayLabel(_ day: String, today: String) -> String {
    guard let date = parseDay(day), let now = parseDay(today) else { return day }
    let calendar = Calendar.current
    let days = calendar.dateComponents([.day], from: date, to: now).day ?? 0
    switch days {
    case 0: return "Today"
    case 1: return "Yesterday"
    default:
        let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: now)
        return sameYear
            ? date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
            : date.formatted(.dateTime.day().month(.abbreviated).year())
    }
}

/// `YYYY-MM-DD` at local midnight.
func parseDay(_ s: String) -> Date? {
    let parts = s.split(separator: "-").compactMap { Int($0) }
    guard parts.count == 3 else { return nil }
    return Calendar.current.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2]))
}

func formatWeight(_ w: Double) -> String {
    w.formatted(.number.precision(.fractionLength(0...2)))
}
