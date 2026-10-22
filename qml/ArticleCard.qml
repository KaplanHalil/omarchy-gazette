import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import Quickshell.Widgets

// Dense article row with optional thumbnail, matching the Omarchy panel kit.
BorderSurface {
  id: root

  property var article: ({})
  property string tabLabel: ""
  property bool selected: false
  property bool compact: false
  property bool showImages: true
  property bool showExcerpts: true
  property bool showMeta: true

  signal activated()
  signal cursorEntered()
  signal bookmarkRequested()

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.64)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.44)
  readonly property string fontFamily: Style.font.family
  readonly property bool unread: article.read !== true
  readonly property bool hasImage: !compact
    && showImages
    && String(article.imageFile || "").length > 0
  readonly property bool excerptVisible: !compact
    && showExcerpts
    && String(article.summary || "").length > 0
  readonly property int pad: Style.space(9)

  radius: Style.cornerRadius
  color: selected
    ? Qt.rgba(accent.r, accent.g, accent.b, 0.10)
    : (mouse.containsMouse ? Style.hoverFillFor(fg, accent) : "transparent")
  borderSpec: selected
    ? Border.flat(accent, 2)
    : Border.controlSpec("normal", fg, accent)
  implicitHeight: compact ? Style.space(40) : (excerptVisible ? Style.space(106) : Style.space(78))

  Behavior on color { ColorAnimation { duration: 80 } }

  Item {
    anchors.fill: parent
    anchors.margins: root.pad

    Column {
      id: textColumn
      anchors.left: parent.left
      anchors.top: parent.top
      anchors.right: imageFrame.visible ? imageFrame.left : parent.right
      anchors.rightMargin: imageFrame.visible ? Style.space(10) : 0
      spacing: Style.space(3)

      Item {
        id: metaRow
        visible: !root.compact && root.showMeta
        width: parent.width
        height: Style.space(16)

        Row {
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(6)

          Rectangle {
            height: Style.space(16)
            width: Math.min(feedText.implicitWidth, Style.space(120)) + Style.space(8)
            radius: Math.min(2, Style.cornerRadius)
            color: Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.10)

            Text {
              id: feedText
              anchors.centerIn: parent
              textFormat: Text.PlainText
              text: String(root.article.feedTitle || "Feed")
              color: root.fg
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
              width: Math.min(implicitWidth, Style.space(120))
            }
          }

          Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            visible: root.tabLabel.length > 0
            text: "·"
            color: root.dimmer
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            visible: root.tabLabel.length > 0
            text: root.tabLabel
            color: root.dimmer
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            elide: Text.ElideRight
            width: Math.min(implicitWidth, Style.space(80))
          }

          Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            visible: root.tabLabel.length > 0
            text: "·"
            color: root.dimmer
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: String(root.article.age || "")
            color: root.dimmer
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }
        }

        Row {
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(6)

          Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            visible: root.article.bookmarked === true
            text: "\uf02e"
            color: root.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            visible: root.unread
            text: "NEW"
            color: root.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            font.bold: true
            font.letterSpacing: 0.6
          }
        }
      }

      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: String(root.article.title || "")
        color: root.unread ? root.fg : root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
        font.bold: root.unread
        wrapMode: Text.Wrap
        maximumLineCount: root.compact ? 1 : 2
        elide: Text.ElideRight
        lineHeight: 1.15
      }

      Text {
        visible: root.excerptVisible
        width: parent.width
        textFormat: Text.PlainText
        text: String(root.article.summary || "")
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        lineHeight: 1.1
      }
    }

    ClippingRectangle {
      id: imageFrame
      visible: root.hasImage
      width: Style.space(96)
      height: Style.space(68)
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      radius: Style.cornerRadius
      color: Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.06)

      Text {
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: "\uf03e"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.heading
      }

      Image {
        id: thumb
        anchors.fill: parent
        source: String(root.article.imageFile || "")
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        smooth: true
        sourceSize.width: 240
        sourceSize.height: 180
        opacity: status === Image.Ready ? 1 : 0

        Behavior on opacity { NumberAnimation { duration: 140 } }
      }
    }
  }

  MouseArea {
    id: mouse
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    onEntered: root.cursorEntered()
    onClicked: function(event) {
      if (event.button === Qt.RightButton) root.bookmarkRequested()
      else root.activated()
    }
  }
}
