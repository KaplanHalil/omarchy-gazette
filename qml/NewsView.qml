import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// News list: custom tabs, search, thumbnails and the keyboard cursor.
Item {
  id: root

  property var panel: null
  property var articles: []
  property var tabs: []
  property var gazetteSettings: ({})
  property string activeTab: "unread"
  property int unreadCount: 0
  property int totalCount: 0
  property int cursor: 0
  property string search: ""
  property bool refreshing: false

  signal tabPicked(string id)
  signal addTabRequested()
  signal searchEdited(string text)
  signal cursorMoved(int index)
  signal activateAt(int index)
  signal refreshRequested()
  signal settingsRequested()
  signal imagesToggled(bool enabled)
  signal clearSearchRequested()

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.62)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: Style.font.family
  readonly property bool searchFocused: searchField.activeFocus
  readonly property bool compact: gazetteSettings.density === "compact"
  readonly property int bookmarked: Number(panel && panel.stats ? panel.stats.bookmarked : 0)
  readonly property int maxRows: 100

  readonly property bool imagesEnabled: {
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].id === activeTab) {
        if (tabs[i].images === true) return true
        if (tabs[i].images === false) return false
      }
    }
    return gazetteSettings.show_images !== false
  }

  function focusSearch() {
    searchField.forceActiveFocus()
  }

  function tabNameFor(article) {
    if (!article || !article.tabs || article.tabs.length === 0) return ""
    return panel ? panel.tabName(article.tabs[0]) : article.tabs[0]
  }

  function ensureCursor() {
    if (articles.length === 0) return
    var index = Math.max(0, Math.min(articles.length - 1, cursor))
    var item = repeater.itemAt(index)
    if (!item) return
    var top = item.y
    var bottom = item.y + item.height
    if (top < list.contentY) list.contentY = top
    else if (bottom > list.contentY + list.height) list.contentY = bottom - list.height
  }

  function emptyMessage() {
    if (panel && panel.feeds.length === 0) return "No feeds yet. Press f to add feeds or the starter pack."
    if (search.length > 0) return "No articles match “" + search + "”."
    if (activeTab === "unread") return "You're all caught up."
    if (activeTab === "bookmarks") return "No bookmarks yet. Press b on an article."
    return "Nothing in this tab yet."
  }

  ColumnLayout {
    anchors.fill: parent
    spacing: Style.space(7)

    Flickable {
      id: tabFlick
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(27)
      contentWidth: tabRow.implicitWidth
      contentHeight: height
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.HorizontalFlick
      interactive: contentWidth > width

      Row {
        id: tabRow
        height: parent.height
        spacing: Style.space(4)

        TabChip {
          label: "Unread"
          count: root.unreadCount
          active: root.activeTab === "unread"
          onClicked: root.tabPicked("unread")
        }

        TabChip {
          label: "All"
          count: root.totalCount
          active: root.activeTab === "all"
          onClicked: root.tabPicked("all")
        }

        Repeater {
          model: root.tabs

          TabChip {
            required property var modelData
            label: String(modelData.name || "")
            glyph: String(modelData.glyph || "")
            count: Number(modelData.unread || 0)
            active: root.activeTab === modelData.id
            onClicked: root.tabPicked(modelData.id)
          }
        }

        TabChip {
          visible: root.bookmarked > 0
          label: "Saved"
          count: root.bookmarked
          active: root.activeTab === "bookmarks"
          onClicked: root.tabPicked("bookmarks")
        }

        TabChip {
          label: "+"
          active: false
          onClicked: root.addTabRequested()
        }
      }
    }

    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(30)
      spacing: Style.space(5)

      Text {
        textFormat: Text.PlainText
        text: "\uf002"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
      }

      TextField {
        id: searchField
        Layout.fillWidth: true
        placeholderText: "search articles…"
        text: root.search
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.bodySmall
        verticalPadding: Style.space(4)

        onTextEdited: root.searchEdited(text)
        onAccepted: {
          if (root.articles.length > 0) root.activateAt(0)
        }
        Keys.onEscapePressed: {
          root.clearSearchRequested()
          searchField.text = ""
          if (root.panel) root.panel.focusList()
        }
      }

      Text {
        visible: root.search.length > 0
        textFormat: Text.PlainText
        text: "ESC"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      PanelActionButton {
        iconText: "\uf03e"
        tooltipText: root.imagesEnabled ? "Hide images" : "Show images"
        foreground: root.imagesEnabled ? root.accent : root.dimmer
        fontFamily: root.fontFamily
        onClicked: root.imagesToggled(!root.imagesEnabled)
      }

      PanelActionButton {
        iconText: "\uf013"
        tooltipText: "Settings"
        foreground: root.dimmer
        fontFamily: root.fontFamily
        onClicked: root.settingsRequested()
      }
    }

    PanelSeparator { Layout.fillWidth: true }

    Flickable {
      id: list
      Layout.fillWidth: true
      Layout.fillHeight: true
      clip: true
      contentWidth: width
      contentHeight: listColumn.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: listColumn
        width: list.width
        spacing: Style.space(5)

        Repeater {
          id: repeater
          model: root.articles.slice(0, root.maxRows)

          ArticleCard {
            required property var modelData
            required property int index

            width: listColumn.width
            article: modelData
            tabLabel: root.tabNameFor(modelData)
            selected: index === root.cursor
            compact: root.compact
            showImages: root.imagesEnabled
            showExcerpts: root.gazetteSettings.show_excerpts !== false
            onActivated: root.activateAt(index)
            onCursorEntered: root.cursorMoved(index)
            onBookmarkRequested: {
              if (root.panel) root.panel.toggleBookmark(modelData)
            }          }
        }

        Item {
          visible: root.articles.length === 0
          width: parent.width
          height: Style.space(150)

          Column {
            anchors.centerIn: parent
            spacing: Style.space(6)

            Text {
              anchors.horizontalCenter: parent.horizontalCenter
              textFormat: Text.PlainText
              text: root.activeTab === "unread" ? "\uf00c" : "\uf09e"
              color: root.dimmer
              font.family: root.fontFamily
              font.pixelSize: Style.font.display
            }

            Text {
              anchors.horizontalCenter: parent.horizontalCenter
              width: Style.space(360)
              horizontalAlignment: Text.AlignHCenter
              wrapMode: Text.Wrap
              textFormat: Text.PlainText
              text: root.emptyMessage()
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.bodySmall
            }

            Text {
              anchors.horizontalCenter: parent.horizontalCenter
              textFormat: Text.PlainText
              text: root.refreshing ? "refreshing feeds…" : "r refresh · f feeds · t tabs"
              color: root.dimmer
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
          }
        }

        Item {
          visible: root.articles.length > root.maxRows
          width: parent.width
          height: Style.space(24)

          Text {
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: "+" + (root.articles.length - root.maxRows) + " more — refine the search"
            color: root.dimmer
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }
        }
      }
    }
  }
}
