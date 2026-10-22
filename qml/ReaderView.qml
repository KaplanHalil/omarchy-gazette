import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import Quickshell.Widgets

// In-panel reader. Feed HTML is converted to plain text by the engine, so the
// panel never renders remote markup.
Item {
  id: root

  property var panel: null
  property var article: null
  property bool showImages: true

  signal backRequested()
  signal openExternalRequested(var article)
  signal toggleBookmarkRequested(var article)
  signal toggleReadRequested(var article)

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.68)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.44)
  readonly property string fontFamily: Style.font.family
  readonly property bool hasImage: showImages
    && article !== null
    && String(article.imageFile || "").length > 0

  function scrollBy(steps) {
    readerFlick.contentY = Math.max(
      0,
      Math.min(readerFlick.contentHeight - readerFlick.height,
        readerFlick.contentY + steps * Style.space(70))
    )
  }

  ColumnLayout {
    anchors.fill: parent
    spacing: Style.space(6)

    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(26)
      spacing: Style.space(6)

      PanelActionButton {
        iconText: "\uf060"
        tooltipText: "Back (Esc)"
        foreground: root.fg
        fontFamily: root.fontFamily
        onClicked: root.backRequested()
      }

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: root.article
          ? String(root.article.feedTitle || "Feed").toUpperCase() + " · " + String(root.article.date || "")
          : ""
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.5
        elide: Text.ElideRight
      }

      PanelActionButton {
        iconText: "\uf02e"
        tooltipText: root.article && root.article.bookmarked ? "Remove bookmark (b)" : "Bookmark (b)"
        foreground: root.article && root.article.bookmarked ? root.accent : root.fg
        fontFamily: root.fontFamily
        onClicked: if (root.article) root.toggleBookmarkRequested(root.article)
      }

      PanelActionButton {
        iconText: "\uf08e"
        tooltipText: "Open in browser (o)"
        foreground: root.fg
        fontFamily: root.fontFamily
        onClicked: if (root.article) root.openExternalRequested(root.article)
      }
    }

    PanelSeparator { Layout.fillWidth: true }

    Flickable {
      id: readerFlick
      Layout.fillWidth: true
      Layout.fillHeight: true
      clip: true
      contentWidth: width
      contentHeight: articleColumn.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: articleColumn
        width: readerFlick.width
        spacing: Style.space(9)

        Text {
          width: parent.width
          textFormat: Text.PlainText
          text: root.article ? String(root.article.title || "") : ""
          color: root.fg
          font.family: root.fontFamily
          font.pixelSize: Style.font.heading
          font.bold: true
          wrapMode: Text.Wrap
          lineHeight: 1.15
        }

        Text {
          width: parent.width
          visible: root.article && (String(root.article.author || "").length > 0)
          textFormat: Text.PlainText
          text: root.article ? String(root.article.author || "") : ""
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }

        ClippingRectangle {
          width: parent.width
          height: Math.min(Style.space(210), width * 0.52)
          visible: root.hasImage
          radius: Style.cornerRadius
          color: Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.06)

          Image {
            anchors.fill: parent
            source: root.article ? String(root.article.imageFile || "") : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            sourceSize.width: 900
            sourceSize.height: 500
          }
        }

        PanelSeparator { width: parent.width; visible: root.hasImage }

        Text {
          width: parent.width
          textFormat: Text.PlainText
          text: root.article
            ? (String(root.article.content || "").length > 0
              ? String(root.article.content)
              : String(root.article.summary || ""))
            : ""
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          wrapMode: Text.Wrap
          lineHeight: 1.4
        }

        PanelSeparator { width: parent.width }

        RowLayout {
          width: parent.width
          spacing: Style.space(6)

          BorderSurface {
            implicitWidth: readLabel.implicitWidth + Style.space(18)
            implicitHeight: Style.space(26)
            radius: Style.cornerRadius
            color: readMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
            borderSpec: Border.controlSpec("normal", root.fg, root.accent)

            Text {
              id: readLabel
              anchors.centerIn: parent
              textFormat: Text.PlainText
              text: root.article && root.article.read ? "MARK UNREAD" : "MARK READ"
              color: root.fg
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.bold: true
              font.letterSpacing: 0.5
            }

            MouseArea {
              id: readMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: if (root.article) root.toggleReadRequested(root.article)
            }
          }

          BorderSurface {
            implicitWidth: openLabel.implicitWidth + Style.space(18)
            implicitHeight: Style.space(26)
            radius: Style.cornerRadius
            color: openMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
            borderSpec: Border.controlSpec("normal", root.accent, root.accent)

            Text {
              id: openLabel
              anchors.centerIn: parent
              textFormat: Text.PlainText
              text: "OPEN IN BROWSER"
              color: root.accent
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.bold: true
              font.letterSpacing: 0.5
            }

            MouseArea {
              id: openMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: if (root.article) root.openExternalRequested(root.article)
            }
          }

          Item { Layout.fillWidth: true }

          Text {
            textFormat: Text.PlainText
            text: "b save · o browser · esc back"
            color: root.dimmer
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }
        }

        Item { width: 1; height: Style.space(4) }
      }
    }
  }
}
