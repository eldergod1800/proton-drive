import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Left sidebar
        Rectangle {
            width: 200
            Layout.fillHeight: true
            color: "#f5f5f5"

            ListView {
                id: sidebar
                anchors.fill: parent
                anchors.margins: 8
                model: ListModel {
                    ListElement { label: "My Files"; path: "/" }
                    ListElement { label: "Computers"; path: "/computers" }
                    ListElement { label: "Sync Folders"; path: "/sync" }
                }
                delegate: ItemDelegate {
                    width: parent.width
                    text: model.label
                    onClicked: fileList.currentPath = model.path
                }
            }
        }

        Rectangle { width: 1; Layout.fillHeight: true; color: "#ddd" }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            ToolBar {
                Layout.fillWidth: true
                RowLayout {
                    anchors.fill: parent
                    TextField {
                        placeholderText: "Search..."
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Upload"
                    }
                }
            }

            ListView {
                id: fileList
                property string currentPath: ""
                Layout.fillWidth: true
                Layout.fillHeight: true
                model: ListModel {}
                delegate: ItemDelegate {
                    width: parent.width
                    text: model.name
                }

                Label {
                    anchors.centerIn: parent
                    text: "Select a folder to browse"
                    visible: fileList.count === 0
                    color: "#999"
                }
            }

            Rectangle {
                Layout.fillWidth: true
                height: 28
                color: "#f0f0f0"
                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    Label {
                        text: "Ready"
                        Layout.fillWidth: true
                        font.pixelSize: 12
                    }
                    Label {
                        text: "daemon: disconnected"
                        font.pixelSize: 12
                    }
                }
            }
        }
    }
}
