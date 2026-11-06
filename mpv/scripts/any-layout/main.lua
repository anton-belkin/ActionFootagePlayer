-- any-layout: the keys work in a Cyrillic keyboard layout too (Russian, Ukrainian, Belarusian).
--
-- mpv gets the character a key types in the current layout, so with the Russian layout Ctrl+p arrives as Ctrl+з,
-- E as У and . as ю, and nothing is bound to them. Here each Cyrillic letter is passed on as the key that types it,
-- the one with the Latin letter on it (US layout): Ctrl+з goes on as Ctrl+p, held down while it is held, so it does
-- whatever Ctrl+p does at that moment: the plugins' keys, mpv's own and the user's input.conf, including the keys a
-- mode binds for a while (a and A in crop mode). A Cyrillic key bound in input.conf keeps its own binding.

local mp = require 'mp'

-- The Cyrillic letter -> the key it is on. ЙЦУКЕН as in Russian (PC and Mac), with Ukrainian і ї є and Belarusian ў
-- where those layouts put them (Belarusian і, on the b key, goes as Ukrainian і, on the s key)
local KEYS = {
    ["й"] = "q", ["ц"] = "w", ["у"] = "e", ["к"] = "r", ["е"] = "t", ["н"] = "y", ["г"] = "u", ["ш"] = "i",
    ["щ"] = "o", ["з"] = "p", ["х"] = "[", ["ъ"] = "]",
    ["ф"] = "a", ["ы"] = "s", ["в"] = "d", ["а"] = "f", ["п"] = "g", ["р"] = "h", ["о"] = "j", ["л"] = "k",
    ["д"] = "l", ["ж"] = ";", ["э"] = "'",
    ["я"] = "z", ["ч"] = "x", ["с"] = "c", ["м"] = "v", ["и"] = "b", ["т"] = "n", ["ь"] = "m", ["б"] = ",",
    ["ю"] = ".", ["ё"] = "`",
    ["і"] = "s", ["ї"] = "]", ["є"] = "'", ["ў"] = "o",
}
-- With Shift: the capital letter -> the shifted key
local SHIFTED = { ["["] = "{", ["]"] = "}", [";"] = ":", ["'"] = '"', [","] = "<", ["."] = ">", ["`"] = "~" }
local UPPER = {
    ["й"] = "Й", ["ц"] = "Ц", ["у"] = "У", ["к"] = "К", ["е"] = "Е", ["н"] = "Н", ["г"] = "Г", ["ш"] = "Ш",
    ["щ"] = "Щ", ["з"] = "З", ["х"] = "Х", ["ъ"] = "Ъ", ["ф"] = "Ф", ["ы"] = "Ы", ["в"] = "В", ["а"] = "А",
    ["п"] = "П", ["р"] = "Р", ["о"] = "О", ["л"] = "Л", ["д"] = "Д", ["ж"] = "Ж", ["э"] = "Э", ["я"] = "Я",
    ["ч"] = "Ч", ["с"] = "С", ["м"] = "М", ["и"] = "И", ["т"] = "Т", ["ь"] = "Ь", ["б"] = "Б", ["ю"] = "Ю",
    ["ё"] = "Ё", ["і"] = "І", ["ї"] = "Ї", ["є"] = "Є", ["ў"] = "Ў",
}
-- Shift is in the character itself (mpv names "Shift+a" A, and gets У from Shift+у), the other modifiers come as such
local MODIFIERS = { "", "Ctrl+", "Alt+", "Meta+", "Ctrl+Alt+" }

local function pass_on(as)
    return function(e)
        if e.event == "down" then
            mp.commandv("keydown", as)
        elseif e.event == "up" and not e.canceled then
            -- Canceled: the keydown just above took over as the key held down (mpv holds one at a time), and the
            -- Cyrillic key's own release lets go of it
            mp.commandv("keyup", as)
        elseif e.event == "press" then
            mp.commandv("keypress", as)
        end
        -- "repeat": the keydown above repeats by itself, as the Latin key would
    end
end

local n = 0
local function bind(cyrillic, latin)
    for _, mod in ipairs(MODIFIERS) do
        n = n + 1
        mp.add_key_binding(mod .. cyrillic, "key-" .. n, pass_on(mod .. latin), { complex = true })
    end
end

for cyrillic, latin in pairs(KEYS) do
    bind(cyrillic, latin)
    bind(UPPER[cyrillic], SHIFTED[latin] or latin:upper())
end
