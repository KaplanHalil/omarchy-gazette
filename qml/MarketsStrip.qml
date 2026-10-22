import QtQuick
import qs.Commons
import qs.Commons as Commons

// Pinned markets ticker. Each chip shows label, price and daily change.
// Horizontal flick when more tickers are enabled than fit.
Item {
  id: root

  property var markets: []
  property real updated: 0
  signal activated()

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: Style.font.family

  implicitHeight: Style.space(30)

  function clock(ts) {
    if (!ts || ts <= 0) return "—"
    var d = new Date(ts * 1000)
    var h = d.getHours()
    var m = d.getMinutes()
    return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m
  }

  Flickable {
    id: stripFlick
    anchors.fill: parent
    contentWidth: stripRow.implicitWidth
    contentHeight: height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.HorizontalFlick
    interactive: contentWidth > width

    Row {
      id: stripRow
      height: parent.height
      spacing: Style.space(9)

      Text {
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: "MARKETS"
        color: root.accent
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.bold: true
        font.letterSpacing: 1.0
      }

      Text {
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: root.clock(root.updated)
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      Repeater {
        model: root.markets

        Item {
          id: chip
          required property var modelData
          required property int index
          width: chipRow.implicitWidth
          height: stripRow.height

          Row {
            id: chipRow
            height: parent.height
            spacing: Style.space(5)

            Rectangle {
              anchors.verticalCenter: parent.verticalCenter
              width: 1
              height: Style.space(12)
              color: root.dimmer
              visible: chip.index > 0
            }

            Text {
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: String(chip.modelData.label || "").toUpperCase()
              color: root.dimmer
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.letterSpacing: 0.4
            }

            Text {
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: String(chip.modelData.priceText || "—")
              color: chip.modelData.stale
                ? Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.55)
                : root.fg
              font.family: root.fontFamily
              font.pixelSize: Style.font.bodySmall
              font.bold: true
            }

            Text {
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: String(chip.modelData.changeText || "")
              color: Number(chip.modelData.changePct) > 0.005
                ? root.accent
                : (Number(chip.modelData.changePct) < -0.005 ? root.urgent : root.dimmer)
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
          }

          MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.activated()
          }
        }
      }
    }

    WheelHandler {
      onWheel: function(event) {
        var max = Math.max(0, stripFlick.contentWidth - stripFlick.width)
        stripFlick.contentX = Math.max(0, Math.min(max, stripFlick.contentX - event.angleDelta.y * 0.6))
        event.accepted = true
      }
    }
  }

  Rectangle {
    visible: stripFlick.contentWidth > stripFlick.width
    anchors.right: parent.right
    width: Style.space(26)
    height: parent.height
    gradient: Gradient {
      orientation: Gradient.Horizontal
      GradientStop { position: 0.0; color: "transparent" }
      GradientStop { position: 1.0; color: Commons.Color.popups.background }
    }

    Text {
      anchors.right: parent.right
      anchors.rightMargin: Style.space(4)
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: "›"
      color: root.dimmer
      font.family: root.fontFamily
      font.pixelSize: Style.font.body
    }
  }
}
