        // Luma settings update this ribbon without restarting the compositor.
        property var lumaMenu: ({"enabled":false,"entries":{}})
        property InputPickerService lumaConfig: InputPickerService {
            service: "com.webos.service.config"
            method: "getConfigs"
            Component.onCompleted: callService({"configNames":["com.ecklf.luma.inputMenu"],"subscribe":true})
            Component.onDestruction: cancel()
            onCallSuccess: inputPickerRoot.lumaAcceptSettings(response)
        }
        function lumaAcceptSettings(response) {
            var value = response && response.configs && response.configs["com.ecklf.luma.inputMenu"];
            if (response && response.returnValue === true && value && typeof value.enabled === "boolean" && value.entries && typeof value.entries === "object") {
                inputPickerRoot.lumaMenu = value;
            }
        }
        onLumaMenuChanged: {
            if (inputPickerMainView && inputPickerMainView.inputPickerListModel && inputPickerMainView.inputPickerListModel.lumaReady)
                inputPickerMainView.inputPickerListModel.updateListModel();
        }
