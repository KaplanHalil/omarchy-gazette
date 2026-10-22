import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// One tab in the horizontally scrolling tab strip.
BorderSurface {
  id: root

  property string label: ""
  property string glyph: ""
  property int count: -1
  property bool active: false

  signal clicked()

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.5)
  readonly property string fontFamily: Style.font.family

  implicitHeight: Style.space(27)
  implicitWidth: tabRow.implicitWidth + Style.space(18)
  radius: Style.cornerRadius
  color: active
    ? Style.selectedFillFor(fg, accent)
    : (mouse.containsMouse ? Style.hoverFillFor(fg, accent) : "transparent")
  borderSpec: Border.controlSpec(active ? "selected" : "normal", fg, accent)

  Behavior on color { ColorAnimation { duration: 80 } }

  Row {
    id: tabRow
    anchors.centerIn: parent
    spacing: Style.space(6)

    Text {
      visible: root.glyph.length > 0
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: root.glyph
      color: root.active ? root.accent : root.dimmer
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }

    Text {
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: root.label
      color: root.active ? root.accent : root.fg
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
      font.bold: root.active
    }

    Text {
      visible: root.count >= 0
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: root.count >= 0 ? String(root.count) : ""
      color: root.dimmer
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }
  }

  MouseArea {
    id: mouse
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    onClicked: root.clicked()
  }
}
