import AppKit
import NotCore

// Task blocks in notes (ADR 0011): a checklist made into tasks shows as one block, drawn from the
// tasks as they are now over its hidden `![](not-tasks:…)` line, like an image. Clicking a box ticks
// the task, clicking a row shows it in its list, "Add Task" adds one.

/// What the editor does with a block's rows; the view hosting it supplies these.
struct TaskBlockActions {
    /// The block as it is now; nil if it can't be read.
    var view: (_ markup: String) -> TaskBlockView?
    var toggle: (TaskItem) -> Void
    var open: (TaskItem) -> Void
    /// "Add Task" in the block whose line is `markup`.
    var add: (_ markup: String) -> Void
}

@MainActor
enum TaskBlockLayout {
    static let pad: CGFloat = 6
    static let header: CGFloat = 24
    static let row: CGFloat = 26
    static let indent: CGFloat = 20
    static let box: CGFloat = 14
    static let inset: CGFloat = 12

    /// The block's height, without the gap around it.
    static func height(_ view: TaskBlockView?) -> CGFloat {
        guard let view, !view.gone else { return row + pad * 2 }
        return pad * 2 + (view.title.isEmpty ? 0 : header) + row * CGFloat(view.rows.count + 1)
    }

    enum Tap {
        case toggle(TaskItem)
        case open(TaskItem)
        case add
    }

    /// What a click at `point` inside `rect` (flipped: y grows down) means.
    static func tap(_ view: TaskBlockView?, in rect: NSRect, at point: NSPoint) -> Tap? {
        guard let view, !view.gone else { return nil }
        let rowsTop = rect.minY + pad + (view.title.isEmpty ? 0 : header)
        guard point.y >= rowsTop else { return nil }
        let index = Int((point.y - rowsTop) / row)
        if index == view.rows.count { return .add }
        guard index < view.rows.count else { return nil }
        let r = view.rows[index]
        let boxRight = rect.minX + inset + indent * CGFloat(r.depth) + box
        return point.x <= boxRight + 8 ? .toggle(r.task) : .open(r.task)
    }

    static func draw(_ view: TaskBlockView?, in rect: NSRect, box drawBox: (NSRect, Bool) -> Void) {
        NSColor.quaternaryLabelColor.withAlphaComponent(0.12).setFill()
        NSBezierPath(roundedRect: rect, xRadius: 10, yRadius: 10).fill()
        func label(_ text: String, x: CGFloat, y: CGFloat, height: CGFloat, attributes: [NSAttributedString.Key: Any]) {
            let string = NSAttributedString(string: text, attributes: attributes)
            let size = string.size()
            let width = max(rect.maxX - inset - x, 1)
            string.draw(
                with: NSRect(x: x, y: y + (height - size.height) / 2, width: width, height: size.height),
                options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine]
            )
        }
        let body = NSFont.systemFont(ofSize: MarkdownStyler.bodySize)
        var y = rect.minY + pad
        guard let view, !view.gone else {
            label("Tasks deleted", x: rect.minX + inset, y: y, height: row, attributes: [.font: body, .foregroundColor: NSColor.tertiaryLabelColor])
            return
        }
        if !view.title.isEmpty {
            label(view.title, x: rect.minX + inset, y: y, height: header, attributes: [
                .font: NSFont.systemFont(ofSize: 12, weight: .semibold), .foregroundColor: NSColor.secondaryLabelColor,
            ])
            y += header
        }
        for r in view.rows {
            let x = rect.minX + inset + indent * CGFloat(r.depth)
            drawBox(NSRect(x: x, y: y + (row - box) / 2, width: box, height: box), r.task.completedAt != nil)
            var attributes: [NSAttributedString.Key: Any] = [.font: body, .foregroundColor: r.done ? NSColor.secondaryLabelColor : NSColor.labelColor]
            if r.done { attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
            label(r.task.title, x: x + box + 8, y: y, height: row, attributes: attributes)
            y += row
        }
        label("+  Add Task", x: rect.minX + inset, y: y, height: row, attributes: [.font: body, .foregroundColor: NSColor.controlAccentColor])
    }
}
