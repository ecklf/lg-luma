// Diagnostic only: this uses the firmware's QML host, not Luma's SDL renderer.
import QtQuick 2.4
import QtQuick.Window 2.2
import Eos.Window 0.1

WebOSWindow {
    id: win
    appId: "com.ecklf.luma.overlayprobe"
    title: "Luma Overlay Probe"
    windowType: "_WEBOS_WINDOW_TYPE_OVERLAY"
    width: Screen.width > 0 && Screen.width < 1920 ? Screen.width : 1920
    height: Screen.height > 0 && Screen.width < 1920 ? Screen.height : 1080
    color: "transparent"
    visible: true

    property var launchParams: typeof params !== "undefined" ? params : null
    property bool initialized: false
    property int selection: 0

    function reveal() {
        console.log("Luma overlay probe: show");
        win.visible = true;
        controls.forceActiveFocus();
        timeout.restart();
    }
    function dismiss(reason) {
        console.log("Luma overlay probe: hide " + reason);
        timeout.stop();
        win.visible = false;
    }

    Component.onCompleted: {
        initialized = true;
        reveal();
    }
    onLaunchParamsChanged: if (initialized) reveal()
    onActiveChanged: console.log("Luma overlay probe: active=" + active)

    // A stuck or unfocused probe must not leave an overlay covering the TV.
    Timer { id: timeout; interval: 20000; onTriggered: win.dismiss("timeout") }

    Item {
        id: controls
        anchors.fill: parent
        focus: true
        Keys.onPressed: {
            console.log("Luma overlay probe: key=" + event.key);
            if (event.key === Qt.Key_Back || event.key === Qt.Key_Escape || event.key === 461) {
                event.accepted = true;
                win.dismiss("back");
            } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
                event.accepted = true;
                win.selection = (win.selection + (event.key === Qt.Key_Right ? 1 : 2)) % 3;
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                event.accepted = true;
                win.dismiss("ok");
            }
        }

        // Most of the window is intentionally untouched, exposing the real card.
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: win.height * 0.06
            width: win.width * 0.8
            height: win.height * 0.3
            radius: 16
            color: "#bb17191c"
            border.color: "#ffffff"

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                y: parent.height * 0.09
                text: "OVERLAY PROBE - not the recent switcher"
                color: "white"
                font.pixelSize: win.height * 0.027
            }
            Row {
                anchors.centerIn: parent
                spacing: win.width * 0.015
                Repeater {
                    model: 3
                    Rectangle {
                        width: win.width * 0.18
                        height: win.height * 0.1
                        radius: 8
                        color: win.selection === index ? "#557ca6" : "#333943"
                        border.color: win.selection === index ? "white" : "transparent"
                        Text {
                            anchors.centerIn: parent
                            text: "Test card " + (index + 1)
                            color: "white"
                            font.pixelSize: win.height * 0.025
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: { win.selection = index; win.dismiss("pointer"); }
                        }
                    }
                }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: parent.height * 0.09
                text: "Left/Right: focus   OK/Back/click: hide   Auto-hide: 20 seconds"
                color: "white"
                font.pixelSize: win.height * 0.021
            }
        }
    }
}
