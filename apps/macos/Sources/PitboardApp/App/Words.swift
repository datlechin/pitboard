import PitboardKit

// Which app this is to the model, whose words and rules that differ by front follow it.
let front = Front.macMenuBar

// What the views draw beside what the snapshot says, in this front's case.
let words = appWords(front: front)

// What only the menu bar app draws.
let macWords = macMenuBarWords()
