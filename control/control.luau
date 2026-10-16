-- pleamar-wm's control centre: what each monitor is, from the session
-- itself (`pleamar-wm --say wm "get …"`), and what is picked, applied there
-- at once and written in session.conf (`pleamar-wm config set layout …`).

local NAMES = { "leader", "columns", "rows", "grid", "leader-right", "scrolling", "marea", "fractal", "free" }
local SHOWN = { "Leader", "Columns", "Rows", "Grid", "Leader right", "Scrolling", "Marea", "Fractal", "Free" }

local function wm(args, done)
    run("pleamar-wm", args, function(out, code)
        if done then done((out or ""):gsub("%s+$", ""), code) end
    end)
end

local function shown(k, l)
    fact["sel." .. k] = l
    text["now." .. k] = l >= 0 and ("now " .. SHOWN[l + 1]) or ""
end

-- Each monitor's name, and its layout now (free counts first).
local function look()
    for k = 0, 3 do
        wm({ "--say", "wm", "get monname." .. k }, function(name, code)
            if k == 0 then fact.reachable = code == 0 end
            fact["has." .. k] = code == 0 and name ~= ""
            text["mon." .. k] = name
            if code ~= 0 or name == "" then return end
            wm({ "--say", "wm", "get free." .. k }, function(free)
                if (tonumber(free) or 0) > 0.5 then return shown(k, 8) end
                wm({ "--say", "wm", "get lay." .. k }, function(lay)
                    shown(k, math.floor(tonumber(lay) or 0))
                end)
            end)
        end)
    end
end

on("go", function(page) fact.page = page end)

on("pick", function(v)
    local k, l = v // 10, v % 10
    local name = text["mon." .. k]
    if name == "" then return end
    shown(k, l)
    -- At once, there…
    if l == 8 then
        wm({ "--say", "wm", "fact free." .. k .. " 1" })
    else
        wm({ "--say", "wm", "fact lay." .. k .. " " .. l, "fact free." .. k .. " 0" })
    end
    -- …and for the next session.
    wm({ "config", "set", "layout", name, NAMES[l + 1] }, function(out, code)
        text.toast = code == 0 and (name .. " starts in " .. SHOWN[l + 1] .. " from now on") or ("Not saved: " .. out)
        fact.toasted = fact.toasted + 1
    end)
end)

wm({ "--version" }, function(out) text.version = out ~= "" and out or "pleamar-wm" end)
look()
-- A layout picked from the strip, or with Super+W, shows here too.
every(2000, look)
