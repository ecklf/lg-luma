    property bool lumaReady: false
    function lumaKey(item) {
        if (item.id === "titleItem") return "home_dashboard";
        if (item.appId === "com.webos.app.livetv") return "live_tv";
        if (item.id === "USB") return "usb";
        if (item.id === "DLNA") return "media_server";
        var hdmi = /hdmi([1-4])$/i.exec(item.appId || "");
        return hdmi ? "HDMI_" + hdmi[1] : "";
    }
    function lumaAllowed(item) {
        var settings = inputPickerRoot.lumaMenu || {enabled:false,entries:{}};
        if (!settings.enabled) return true;
        var key = lumaKey(item);
        return !key || !settings.entries || settings.entries[key] !== false;
    }
    function lumaRebuild(show) {
        var ribbon = inputPickerMainView.mainRibbon;
        var selected = ribbon.currentIndex >= 0 && ribbon.currentIndex < count ? get(ribbon.currentIndex) : null;
        var identity = selected ? selected.separator : "";
        var items = [];
        var title = inputPickerRoot.inputPickerUtils.titleItemObj[0];
        if (lumaAllowed(title)) items.push(title);
        var devices = reOrderListModel();
        for (var i = 0; i < devices.length; i++) if (lumaAllowed(devices[i])) items.push(devices[i]);
        inputPickerRoot.styler.lumaVisibleCount = items.length;
        clear();
        var selectedIndex = -1;
        for (var j = 0; j < items.length; j++) {
            items[j].hasCategory = j === 0 ? items[j].id !== "titleItem" : items[j].inputType !== items[j-1].inputType;
            append(items[j]);
            if (items[j].separator === identity) selectedIndex = j;
        }
        if (!count) { inputPickerRoot.close(); return; }
        ribbon.currentIndex = selectedIndex >= 0 ? selectedIndex : inputPickerRoot.getLastInputIndex();
        if (show) inputPickerRoot.showView();
    }
