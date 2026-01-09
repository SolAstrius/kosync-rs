local ConfirmBox = require("ui/widget/confirmbox")
local Device = require("device")
local Dispatcher = require("dispatcher")
local Event = require("ui/event")
local InfoMessage = require("ui/widget/infomessage")
local Math = require("optmath")
local MultiInputDialog = require("ui/widget/multiinputdialog")
local NetworkMgr = require("ui/network/manager")
local Notification = require("ui/widget/notification")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local logger = require("logger")
local md5 = require("ffi/sha2").md5
local random = require("random")
local time = require("ui/time")
local util = require("util")
local T = require("ffi/util").template
local _ = require("gettext")

if G_reader_settings:hasNot("device_id") then
    G_reader_settings:saveSetting("device_id", random.uuid())
end

local KOSyncExt = WidgetContainer:extend{
    name = "kosync-ext",
    is_doc_only = true,
    title = _("Register/login to KOSync Extended server"),

    push_timestamp = nil,
    pull_timestamp = nil,
    annotations_version = nil,
    deleted_annotations = nil,

    settings = nil,
}

local SYNC_STRATEGY = {
    PROMPT  = 1,
    SILENT  = 2,
    DISABLE = 3,
}

local CHECKSUM_METHOD = {
    BINARY = 0,
    FILENAME = 1
}

local API_CALL_DEBOUNCE_DELAY = time.s(25)

KOSyncExt.default_settings = {
    custom_server = nil,
    username = nil,
    userkey = nil,
    auto_sync = false,
    sync_annotations = true,
    pages_before_update = nil,  -- nil = disabled, number = sync every N pages
    sync_forward = SYNC_STRATEGY.PROMPT,
    sync_backward = SYNC_STRATEGY.DISABLE,
    checksum_method = CHECKSUM_METHOD.BINARY,
}

function KOSyncExt:init()
    self.push_timestamp = 0
    self.pull_timestamp = 0
    self.annotations_version = 0
    self.deleted_annotations = {}

    -- Page update tracking
    self.last_page = nil
    self.page_update_counter = 0
    self.periodic_push_scheduled = false
    self.last_page_turn_timestamp = 0

    self.settings = G_reader_settings:readSetting("kosync_ext", self.default_settings)
    self.device_id = G_reader_settings:readSetting("device_id")

    self.ui.menu:registerToMainMenu(self)
end

function KOSyncExt:addToMainMenu(menu_items)
    menu_items.progress_sync = {
        text = _("Progress sync"),
        sub_item_table = {
            {
                text = _("Custom sync server"),
                keep_menu_open = true,
                tap_input_func = function()
                    return {
                        title = _("Custom sync server address"),
                        input = self.settings.custom_server or "http://",
                        callback = function(input)
                            self:setCustomServer(input)
                        end,
                    }
                end,
            },
            {
                text_func = function()
                    return self.settings.userkey and (_("Logout"))
                        or _("Register") .. " / " .. _("Login")
                end,
                keep_menu_open = true,
                callback_func = function()
                    if self.settings.userkey then
                        return function(menu)
                            self:logout(menu)
                        end
                    else
                        return function(menu)
                            self:login(menu)
                        end
                    end
                end,
                separator = true,
            },
            {
                text = _("Auto sync on open/close"),
                checked_func = function() return self.settings.auto_sync end,
                callback = function()
                    self.settings.auto_sync = not self.settings.auto_sync
                    self:registerEvents()
                end,
            },
            {
                text_func = function()
                    return T(_("Sync every # pages (%1)"), self:getSyncPeriod())
                end,
                enabled_func = function() return self.settings.auto_sync end,
                help_text = _([[Periodically sync progress while reading. Only works if network is already connected.]]),
                keep_menu_open = true,
                callback = function(touchmenu_instance)
                    local SpinWidget = require("ui/widget/spinwidget")
                    local curr_value = self.settings.pages_before_update or 0
                    local spin_widget = SpinWidget:new{
                        value = curr_value,
                        value_min = 0,
                        value_max = 999,
                        value_step = 1,
                        value_hold_step = 10,
                        ok_text = _("Set"),
                        ok_always_enabled = true,
                        title_text = _("Sync every # pages"),
                        info_text = _("Set to 0 to disable periodic sync."),
                        default_value = 0,
                        callback = function(spin)
                            local value = spin.value
                            if value == 0 then
                                self.settings.pages_before_update = nil
                            else
                                self.settings.pages_before_update = value
                            end
                            if touchmenu_instance then touchmenu_instance:updateItems() end
                        end,
                    }
                    UIManager:show(spin_widget)
                end,
            },
            {
                text = _("Sync annotations (bookmarks, highlights, notes)"),
                checked_func = function() return self.settings.sync_annotations end,
                callback = function()
                    self.settings.sync_annotations = not self.settings.sync_annotations
                end,
                separator = true,
            },
            {
                text = _("Push all data now"),
                enabled_func = function()
                    return self.settings.userkey ~= nil
                end,
                callback = function()
                    self:pushAll(true)
                end,
            },
            {
                text = _("Pull all data now"),
                enabled_func = function()
                    return self.settings.userkey ~= nil
                end,
                callback = function()
                    self:pullAll(true)
                end,
                separator = true,
            },
            {
                text = _("Document matching method"),
                sub_item_table = {
                    {
                        text = _("Binary (MD5 hash)"),
                        checked_func = function()
                            return self.settings.checksum_method == CHECKSUM_METHOD.BINARY
                        end,
                        callback = function()
                            self.settings.checksum_method = CHECKSUM_METHOD.BINARY
                        end,
                    },
                    {
                        text = _("Filename"),
                        checked_func = function()
                            return self.settings.checksum_method == CHECKSUM_METHOD.FILENAME
                        end,
                        callback = function()
                            self.settings.checksum_method = CHECKSUM_METHOD.FILENAME
                        end,
                    },
                }
            },
        }
    }
end

function KOSyncExt:setCustomServer(server)
    self.settings.custom_server = server ~= "" and server or nil
end

function KOSyncExt:getSyncPeriod()
    if not self.settings.auto_sync then
        return _("N/A")
    end
    if not self.settings.pages_before_update then
        return _("off")
    end
    return tostring(self.settings.pages_before_update)
end

function KOSyncExt:onReaderReady()
    if self.settings.auto_sync then
        UIManager:nextTick(function()
            self:pullAll(false)
        end)
    end
    self:registerEvents()
    self.last_page = self.ui:getCurrentPage()
end

function KOSyncExt:registerEvents()
    if self.settings.auto_sync then
        self.onCloseDocument = self._onCloseDocument
        self.onPageUpdate = self._onPageUpdate
        self.onResume = self._onResume
        self.onSuspend = self._onSuspend
        self.onNetworkConnected = self._onNetworkConnected
        self.onNetworkDisconnecting = self._onNetworkDisconnecting
    else
        self.onCloseDocument = nil
        self.onPageUpdate = nil
        self.onResume = nil
        self.onSuspend = nil
        self.onNetworkConnected = nil
        self.onNetworkDisconnecting = nil
    end
end

function KOSyncExt:_onCloseDocument()
    logger.dbg("KOSyncExt: onCloseDocument")
    -- Disable other handlers to prevent duplicate syncs
    self.onResume = nil
    self.onSuspend = nil
    self.onNetworkConnected = nil
    self.onNetworkDisconnecting = nil
    self.onPageUpdate = nil

    NetworkMgr:goOnlineToRun(function()
        self:pushAll(false)
    end)
end

function KOSyncExt:_onPageUpdate(page)
    if page == nil then
        return
    end

    if self.last_page ~= page then
        self.last_page = page
        self.last_page_turn_timestamp = os.time()
        self.page_update_counter = self.page_update_counter + 1

        -- Schedule push if we've reached the page threshold or one is already scheduled
        if self.periodic_push_scheduled or
           (self.settings.pages_before_update and self.page_update_counter >= self.settings.pages_before_update) then
            self:schedulePeriodicPush()
        end
    end
end

function KOSyncExt:schedulePeriodicPush()
    -- Only sync if network is already up (don't trigger wifi connection)
    if not NetworkMgr:isOnline() then
        return
    end

    self.periodic_push_scheduled = true

    -- Debounce: wait for user to stop turning pages
    UIManager:unschedule(self.doPushCallback)
    self.doPushCallback = function()
        -- Only push if enough time has passed since last page turn (user is idle)
        if os.time() - self.last_page_turn_timestamp >= 3 then
            self.periodic_push_scheduled = false
            self.page_update_counter = 0
            self:pushAll(false)
        else
            -- User still turning pages, reschedule
            UIManager:scheduleIn(3, self.doPushCallback)
        end
    end
    UIManager:scheduleIn(3, self.doPushCallback)
end

function KOSyncExt:_onResume()
    logger.dbg("KOSyncExt: onResume")
    -- Pull progress when resuming from suspend
    UIManager:scheduleIn(1, function()
        self:pullAll(false)
    end)
end

function KOSyncExt:_onSuspend()
    logger.dbg("KOSyncExt: onSuspend")
    -- Push progress before suspending (network should still be up)
    self:pushAll(false)
end

function KOSyncExt:_onNetworkConnected()
    logger.dbg("KOSyncExt: onNetworkConnected")
    -- Pull when network comes up
    UIManager:scheduleIn(0.5, function()
        self:pullAll(false)
    end)
end

function KOSyncExt:_onNetworkDisconnecting()
    logger.dbg("KOSyncExt: onNetworkDisconnecting")
    -- Push before network goes down
    self:pushAll(false)
end

-- === Auth ===

function KOSyncExt:login(menu)
    if NetworkMgr:willRerunWhenOnline(function() self:login(menu) end) then
        return
    end

    local dialog
    dialog = MultiInputDialog:new{
        title = self.title,
        fields = {
            {
                text = self.settings.username,
                hint = "username",
            },
            {
                hint = "password",
                text_type = "password",
            },
        },
        buttons = {
            {
                {
                    text = _("Cancel"),
                    id = "close",
                    callback = function()
                        UIManager:close(dialog)
                    end,
                },
                {
                    text = _("Login"),
                    callback = function()
                        local username, password = unpack(dialog:getFields())
                        username = util.trim(username)
                        if username == "" or password == "" then
                            UIManager:show(InfoMessage:new{
                                text = _("Invalid username or password"),
                                timeout = 2,
                            })
                            return
                        end
                        UIManager:close(dialog)
                        UIManager:scheduleIn(0.5, function()
                            self:doLogin(username, password, menu)
                        end)
                    end,
                },
                {
                    text = _("Register"),
                    callback = function()
                        local username, password = unpack(dialog:getFields())
                        username = util.trim(username)
                        if username == "" or password == "" then
                            UIManager:show(InfoMessage:new{
                                text = _("Invalid username or password"),
                                timeout = 2,
                            })
                            return
                        end
                        UIManager:close(dialog)
                        UIManager:scheduleIn(0.5, function()
                            self:doRegister(username, password, menu)
                        end)
                    end,
                },
            },
        },
    }
    UIManager:show(dialog)
    dialog:onShowKeyboard()
end

function KOSyncExt:doRegister(username, password, menu)
    local KOSyncExtClient = require("KOSyncExtClient")
    local client = KOSyncExtClient:new{
        custom_url = self.settings.custom_server,
        service_spec = self.path .. "/api.json"
    }
    Device:setIgnoreInput(true)
    local userkey = md5(password)
    local ok, status, body = pcall(client.register, client, username, userkey)
    if not ok then
        UIManager:show(InfoMessage:new{
            text = _("Registration failed: ") .. tostring(status),
        })
    elseif status then
        self.settings.username = username
        self.settings.userkey = userkey
        if menu then menu:updateItems() end
        UIManager:show(InfoMessage:new{
            text = _("Registered successfully."),
        })
    else
        UIManager:show(InfoMessage:new{
            text = body and body.message or _("Registration failed"),
        })
    end
    Device:setIgnoreInput(false)
end

function KOSyncExt:doLogin(username, password, menu)
    local KOSyncExtClient = require("KOSyncExtClient")
    local client = KOSyncExtClient:new{
        custom_url = self.settings.custom_server,
        service_spec = self.path .. "/api.json"
    }
    Device:setIgnoreInput(true)
    local userkey = md5(password)
    local ok, status, body = pcall(client.authorize, client, username, userkey)
    if not ok then
        UIManager:show(InfoMessage:new{
            text = _("Login failed: ") .. tostring(status),
        })
    elseif status then
        self.settings.username = username
        self.settings.userkey = userkey
        if menu then menu:updateItems() end
        UIManager:show(InfoMessage:new{
            text = _("Logged in successfully."),
        })
    else
        UIManager:show(InfoMessage:new{
            text = body and body.message or _("Login failed"),
        })
    end
    Device:setIgnoreInput(false)
end

function KOSyncExt:logout(menu)
    self.settings.userkey = nil
    if menu then menu:updateItems() end
end

-- === Document helpers ===

function KOSyncExt:getDocumentDigest()
    if self.settings.checksum_method == CHECKSUM_METHOD.FILENAME then
        local file = self.ui.document.file
        if not file then return end
        local _, file_name = util.splitFilePathName(file)
        return md5(file_name)
    else
        return self.ui.doc_settings:readSetting("partial_md5_checksum")
    end
end

function KOSyncExt:getLastPercent()
    if self.ui.document.info.has_pages then
        return Math.roundPercent(self.ui.paging:getLastPercent())
    else
        return Math.roundPercent(self.ui.rolling:getLastPercent())
    end
end

function KOSyncExt:getLastProgress()
    if self.ui.document.info.has_pages then
        return self.ui.paging:getLastProgress()
    else
        return self.ui.rolling:getLastProgress()
    end
end

function KOSyncExt:syncToProgress(progress)
    if self.ui.document.info.has_pages then
        self.ui:handleEvent(Event:new("GotoPage", tonumber(progress)))
    else
        self.ui:handleEvent(Event:new("GotoXPointer", progress))
    end
end

-- === Push/Pull All ===

function KOSyncExt:pushAll(interactive)
    logger.info("KOSyncExt: pushAll called, interactive:", interactive)
    if not self.settings.username or not self.settings.userkey then
        logger.warn("KOSyncExt: not logged in")
        if interactive then
            UIManager:show(InfoMessage:new{
                text = _("Please login first."),
                timeout = 2,
            })
        end
        return
    end

    logger.info("KOSyncExt: pushing progress...")
    self:pushProgress(interactive)
    logger.info("KOSyncExt: sync_annotations =", self.settings.sync_annotations)
    if self.settings.sync_annotations then
        logger.info("KOSyncExt: pushing annotations...")
        self:pushAnnotations(interactive)
    end
end

function KOSyncExt:pullAll(interactive)
    if not self.settings.username or not self.settings.userkey then
        if interactive then
            UIManager:show(InfoMessage:new{
                text = _("Please login first."),
                timeout = 2,
            })
        end
        return
    end

    self:pullProgress(interactive)
    if self.settings.sync_annotations then
        self:pullAnnotations(interactive)
    end
end

-- === Progress sync ===

function KOSyncExt:pushProgress(interactive)
    local KOSyncExtClient = require("KOSyncExtClient")
    local client = KOSyncExtClient:new{
        custom_url = self.settings.custom_server,
        service_spec = self.path .. "/api.json"
    }
    local doc_digest = self:getDocumentDigest()
    local progress = self:getLastProgress()
    local percentage = self:getLastPercent()

    local ok, err = pcall(client.update_progress,
        client,
        self.settings.username,
        self.settings.userkey,
        doc_digest,
        progress,
        percentage,
        Device.model,
        self.device_id,
        function(ok, body)
            logger.dbg("KOSyncExt: push progress", percentage * 100, "%")
            if interactive then
                if ok then
                    Notification:notify(_("Progress pushed."))
                else
                    UIManager:show(InfoMessage:new{
                        text = _("Failed to push progress."),
                        timeout = 2,
                    })
                end
            end
        end)
    if not ok and interactive then
        UIManager:show(InfoMessage:new{
            text = _("Failed to push progress: ") .. tostring(err),
            timeout = 2,
        })
    end
end

function KOSyncExt:pullProgress(interactive)
    local KOSyncExtClient = require("KOSyncExtClient")
    local client = KOSyncExtClient:new{
        custom_url = self.settings.custom_server,
        service_spec = self.path .. "/api.json"
    }
    local doc_digest = self:getDocumentDigest()

    local ok, err = pcall(client.get_progress,
        client,
        self.settings.username,
        self.settings.userkey,
        doc_digest,
        function(ok, body)
            if not ok or not body or not body.percentage then
                if interactive then
                    UIManager:show(InfoMessage:new{
                        text = _("No progress found on server."),
                        timeout = 2,
                    })
                end
                return
            end

            if body.device == Device.model and body.device_id == self.device_id then
                if interactive then
                    Notification:notify(_("Already at latest progress."))
                end
                return
            end

            local percentage = self:getLastPercent()
            if math.abs(percentage - body.percentage) < 0.001 then
                if interactive then
                    Notification:notify(_("Progress already synced."))
                end
                return
            end

            if interactive then
                self:syncToProgress(body.progress)
                Notification:notify(_("Progress synced."))
            else
                UIManager:show(ConfirmBox:new{
                    text = T(_("Sync to %1% from device '%2'?"),
                             Math.round(body.percentage * 100),
                             body.device or "unknown"),
                    ok_callback = function()
                        self:syncToProgress(body.progress)
                    end,
                })
            end
        end)
    if not ok and interactive then
        UIManager:show(InfoMessage:new{
            text = _("Failed to pull progress: ") .. tostring(err),
            timeout = 2,
        })
    end
end

-- === Annotations sync ===

function KOSyncExt:getLocalAnnotations()
    if self.ui.annotation and self.ui.annotation.annotations then
        return self.ui.annotation.annotations
    end
    return {}
end

function KOSyncExt:setLocalAnnotations(annotations)
    if self.ui.annotation then
        self.ui.annotation.annotations = annotations
        self.ui.annotation:sortItems(annotations)
    end
end

function KOSyncExt:pushAnnotations(interactive)
    logger.info("KOSyncExt: pushAnnotations START")
    local KOSyncExtClient = require("KOSyncExtClient")
    logger.info("KOSyncExt: got client module")
    local client = KOSyncExtClient:new{
        custom_url = self.settings.custom_server,
        service_spec = self.path .. "/api.json"
    }
    logger.info("KOSyncExt: created client, path:", self.path)
    local doc_digest = self:getDocumentDigest()
    logger.info("KOSyncExt: doc_digest:", doc_digest)
    local annotations = self:getLocalAnnotations()
    logger.info("KOSyncExt: annotations count:", #annotations)

    -- Debug: dump structure of first annotation
    if #annotations > 0 then
        local first = annotations[1]
        logger.info("KOSyncExt: first annotation keys:")
        for k, v in pairs(first) do
            logger.info("  ", k, "=", type(v), ":", tostring(v):sub(1, 100))
        end
    end

    -- Try to serialize manually to test
    local JSON = require("json")
    local ok_json, json_str = pcall(JSON.encode, {annotations = annotations, deleted = self.deleted_annotations or {}})
    logger.info("KOSyncExt: JSON encode test:", ok_json, json_str and #json_str or "nil")
    if not ok_json then
        logger.info("KOSyncExt: JSON encode error:", json_str)
    end

    logger.info("KOSyncExt: calling update_annotations...")
    local ok, err = pcall(client.update_annotations,
        client,
        self.settings.username,
        self.settings.userkey,
        doc_digest,
        annotations,
        self.deleted_annotations or {},
        self.annotations_version,
        function(ok, body)
            logger.info("KOSyncExt: callback received, ok:", ok, "body:", body)
            logger.dbg("KOSyncExt: push annotations", #annotations, "items")
            if ok and body then
                self.annotations_version = body.version or 0
                self.deleted_annotations = {}
            end
            if interactive then
                if ok then
                    Notification:notify(T(_("Pushed %1 annotations."), #annotations))
                else
                    UIManager:show(InfoMessage:new{
                        text = _("Failed to push annotations."),
                        timeout = 2,
                    })
                end
            end
        end)
    logger.info("KOSyncExt: pcall returned, ok:", ok, "err:", err)
    if not ok then
        logger.warn("KOSyncExt: pushAnnotations FAILED:", err)
        if interactive then
            UIManager:show(InfoMessage:new{
                text = _("Failed to push annotations: ") .. tostring(err),
                timeout = 2,
            })
        end
    end
end

function KOSyncExt:pullAnnotations(interactive)
    local KOSyncExtClient = require("KOSyncExtClient")
    local client = KOSyncExtClient:new{
        custom_url = self.settings.custom_server,
        service_spec = self.path .. "/api.json"
    }
    local doc_digest = self:getDocumentDigest()

    local ok, err = pcall(client.get_annotations,
        client,
        self.settings.username,
        self.settings.userkey,
        doc_digest,
        function(ok, body)
            if not ok or not body then
                if interactive then
                    UIManager:show(InfoMessage:new{
                        text = _("Failed to pull annotations."),
                        timeout = 2,
                    })
                end
                return
            end

            local server_annotations = body.annotations or {}
            local server_deleted = body.deleted or {}
            local server_version = body.version or 0

            if server_version == self.annotations_version and #server_annotations == 0 then
                if interactive then
                    Notification:notify(_("No new annotations."))
                end
                return
            end

            local local_annotations = self:getLocalAnnotations()
            local merged = self:mergeAnnotations(local_annotations, server_annotations, server_deleted)

            self:setLocalAnnotations(merged)
            self.annotations_version = server_version

            if interactive then
                Notification:notify(T(_("Synced %1 annotations."), #merged))
            end
        end)
    if not ok and interactive then
        UIManager:show(InfoMessage:new{
            text = _("Failed to pull annotations: ") .. tostring(err),
            timeout = 2,
        })
    end
end

function KOSyncExt:mergeAnnotations(local_annos, server_annos, server_deleted)
    -- Build position key for matching
    local function positionKey(a)
        local page_str = type(a.page) == "string" and a.page or tostring(a.page)
        local pos0_str = a.pos0 and tostring(a.pos0) or ""
        local pos1_str = a.pos1 and tostring(a.pos1) or ""
        return page_str .. "|" .. pos0_str .. "|" .. pos1_str
    end

    local function effectiveTime(a)
        return a.datetime_updated or a.datetime or ""
    end

    -- Index server annotations
    local server_by_key = {}
    for _, a in ipairs(server_annos) do
        server_by_key[positionKey(a)] = a
    end

    -- Build deleted set
    local deleted_set = {}
    for _, d in ipairs(server_deleted) do
        deleted_set[d] = true
    end

    local merged = {}

    -- Process local annotations
    for _, local_a in ipairs(local_annos) do
        local key = positionKey(local_a)
        local server_a = server_by_key[key]

        if deleted_set[local_a.datetime] then
            -- Deleted on server, skip
        elseif server_a then
            -- Exists on both, keep newer
            if effectiveTime(local_a) >= effectiveTime(server_a) then
                table.insert(merged, local_a)
            else
                table.insert(merged, server_a)
            end
            server_by_key[key] = nil -- Mark as processed
        else
            -- Local only, keep
            table.insert(merged, local_a)
        end
    end

    -- Add remaining server-only annotations
    for _, server_a in pairs(server_by_key) do
        if not self.deleted_annotations or not self.deleted_annotations[server_a.datetime] then
            table.insert(merged, server_a)
        end
    end

    return merged
end

-- Track deleted annotations for sync
function KOSyncExt:onAnnotationDeleted(item)
    if item and item.datetime then
        self.deleted_annotations = self.deleted_annotations or {}
        table.insert(self.deleted_annotations, item.datetime)
    end
end

return KOSyncExt
