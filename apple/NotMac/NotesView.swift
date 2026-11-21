import NotCore
import SwiftUI

/// Notes of the selected folder beside the editor of the selected note.
struct NotesView: View {
    @Bindable var model: AppModel

    var body: some View {
        HSplitView {
            NoteList(model: model)
                .frame(minWidth: 220, idealWidth: 270, maxWidth: 380)
            Group {
                if let note = model.selectedNote {
                    NoteEditor(model: model, note: note)
                        .id(note.id)
                } else {
                    ContentUnavailableView {
                        Label("No Note Selected", systemImage: "note.text")
                    } actions: {
                        Button("New Note") { model.newNote() }
                    }
                }
            }
            .frame(minWidth: 360, maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle(model.folderTitle(model.selectedFolderID))
        .searchable(text: $model.noteSearch, placement: .toolbar, prompt: "Search notes")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                let sort = model.noteSort
                Menu {
                    Picker("Sort By", selection: Binding(get: { sort }, set: model.setNoteSort)) {
                        // By hand only in a folder: All Notes mixes every folder.
                        ForEach(model.selectedFolderID == nil ? [NoteSort.edited, .created, .title] : [.edited, .created, .title, .manual], id: \.self) {
                            Text($0.label).tag($0)
                        }
                    }
                    .pickerStyle(.inline)
                } label: {
                    Label("Sort", systemImage: "arrow.up.arrow.down")
                }
                .help(model.selectedFolderID == nil ? "Sort notes: \(sort.label.lowercased())"
                    : "Sort notes: \(sort.label.lowercased()) — dragging a note sorts by hand")
                if model.selectedNote != nil {
                    Button(action: model.pickImagesForNote) {
                        Label("Add Image", systemImage: "photo.badge.plus")
                    }
                    .help("Add images to the note (or paste or drop them in)")
                }
                if let note = model.selectedNote {
                    Button { model.togglePin(note) } label: {
                        Label(note.pinned ? "Unpin" : "Pin", systemImage: note.pinned ? "pin.slash" : "pin")
                    }
                    .help(note.pinned ? "Unpin" : "Pin to the top")
                }
                Button(action: model.newNote) {
                    Label("New Note", systemImage: "square.and.pencil")
                }
                .help("New note (⌘N)")
            }
        }
    }
}

private struct NoteList: View {
    @Bindable var model: AppModel

    var body: some View {
        if model.notes.isEmpty {
            ContentUnavailableView(
                model.noteSearch.isEmpty ? "No Notes" : "No Results",
                systemImage: model.noteSearch.isEmpty ? "note.text" : "magnifyingglass",
                description: Text(model.noteSearch.isEmpty ? "Press ⌘N to write one." : "")
            )
        } else {
            List(selection: $model.selectedNoteID) {
                ForEach(model.notes, id: \.id) { note in
                    NoteRow(note: note, showsFolder: model.selectedFolderID == nil, model: model)
                        .tag(note.id)
                        .draggable(note.id)
                }
                // Dropping between notes of a folder sorts it by hand. All Notes mixes folders, so
                // it stays sorted by latest edit.
                .dropDestination(for: String.self) { ids, index in
                    model.dropNotes(ids, at: index)
                }
            }
            .listStyle(.inset)
            .animation(.snappy, value: model.notes.map(\.id))
            .contextMenu(forSelectionType: String.self) { ids in
                if let id = ids.first, let note = model.notes.first(where: { $0.id == id }) {
                    Button(note.pinned ? "Unpin" : "Pin") { model.togglePin(note) }
                    Menu("Move To") {
                        Button("Notes (No Folder)") { model.moveNote(id, to: nil) }
                        ForEach(model.folders, id: \.id) { folder in
                            Button(String(repeating: "    ", count: Int(folder.depth)) + folder.title) {
                                model.moveNote(id, to: folder.id)
                            }
                        }
                    }
                    Divider()
                    Button("Delete", role: .destructive) { model.deleteNote(id) }
                }
            }
            .onDeleteCommand { if let id = model.selectedNoteID { model.deleteNote(id) } }
        }
    }
}

private struct NoteRow: View {
    let note: NoteSummary
    let showsFolder: Bool
    let model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 4) {
                if note.pinned {
                    Image(systemName: "pin.fill").imageScale(.small).foregroundStyle(.orange)
                }
                Text(note.title.isEmpty ? "New Note" : note.title)
                    .font(.headline)
                    .foregroundStyle(note.title.isEmpty ? .secondary : .primary)
                    .lineLimit(1)
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(edited(note.updatedAt))
                    .foregroundStyle(.secondary)
                NotePreviewLine(preview: note.preview)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            .font(.callout)
            if showsFolder, let folder = note.folderId {
                Label(model.folderTitle(folder), systemImage: "folder")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 4)
    }
}

private struct NoteEditor: View {
    let model: AppModel
    let note: NoteSummary
    @State private var text = ""
    /// What the store had when last loaded or saved, to tell local edits from synced ones.
    @State private var saved = ""

    var body: some View {
        VStack(spacing: 0) {
            Text(Date(timeIntervalSince1970: TimeInterval(note.updatedAt) / 1000)
                .formatted(date: .long, time: .shortened))
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(.top, 10)
            MarkdownEditor(
                text: $text,
                focusRequests: model.editorFocusRequests,
                taskTags: model.tags.map(\.name),
                onMakeTasks: { range, tag in
                    let end = range.location + range.length
                    // The text stays as it is; the body was saved with the tasks.
                    if model.checklistToTasks(note.id, text, start: range.location, end: end, tag: tag) {
                        saved = text
                    }
                },
                onAddImage: model.addImage,
                imageToInsert: model.imageToInsert,
                imageVersion: model.imageVersion
            )
        }
        .onAppear {
            text = model.openNote(note.id)
            saved = text
        }
        // Saving pauses briefly after typing stops.
        .task(id: text) {
            guard text != saved else { return }
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            saved = text
            model.setNoteBody(note.id, text)
        }
        .onChange(of: note.updatedAt) {
            // Edited on another device: take it unless there are unsaved local changes.
            let body = model.noteBody(note.id)
            if text == saved, body != text {
                text = body
                saved = body
            }
        }
        .onDisappear {
            if text != saved { model.setNoteBody(note.id, text) }
            model.discardIfEmpty(note.id)
        }
    }
}

func edited(_ millis: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
    return Calendar.current.isDateInToday(date)
        ? date.formatted(date: .omitted, time: .shortened)
        : date.formatted(date: .numeric, time: .omitted)
}

/// The first line of a note's preview; checklist items (marked by the core) get a box.
struct NotePreviewLine: View {
    let preview: String

    var body: some View {
        let line = preview.split(separator: "\n").first.map(String.init) ?? ""
        if line.isEmpty {
            Text("No additional text")
        } else if line.hasPrefix("☐ ") {
            Label { Text(line.dropFirst(2)) } icon: { Image(systemName: "square") }
                .labelStyle(.titleAndIcon)
        } else if line.hasPrefix("☑ ") {
            Label { Text(line.dropFirst(2)).strikethrough() } icon: { Image(systemName: "checkmark.square.fill") }
                .labelStyle(.titleAndIcon)
        } else {
            Text(line)
        }
    }
}

extension NoteSort {
    var label: String {
        switch self {
        case .edited: "Last Edited"
        case .created: "Date Created"
        case .title: "Title"
        case .manual: "Manual"
        }
    }
}
