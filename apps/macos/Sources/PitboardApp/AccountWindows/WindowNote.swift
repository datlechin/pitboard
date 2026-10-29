import Foundation
import PitboardKit

struct WindowNote: Equatable, Identifiable {
    let kind: WindowNoteKind
    let text: String
    /// Tells two notes of the same kind apart, so saying one again shows it again.
    let id = UUID()

    init(_ kind: WindowNoteKind, for account: WindowAccount) {
        self.kind = kind
        text = windowNote(front: front, kind: kind, account: account)
    }

    static func == (lhs: WindowNote, rhs: WindowNote) -> Bool { lhs.id == rhs.id }
}
