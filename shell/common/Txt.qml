import QtQuick

// Text in the shell's font. Lowercase labels are the house style.
Text {
    color: Theme.text
    font.family: Theme.font
    font.pixelSize: Theme.fontSize
    elide: Text.ElideRight
    verticalAlignment: Text.AlignVCenter
}
