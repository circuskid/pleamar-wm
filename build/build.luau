-- pleamar-wm's Build: the logic. It is in three pieces:
--   logic/engine.luau     the AI that builds (agent/, in its sandbox), and the conversation
--   logic/creations.luau  what is built: its files, versions, rules, cards, apps, keeping, sharing
--   logic/draw.luau       the mode, the pen and the box, and Done
local home_dir = sys.ask("env", "HOME") or ""
local shared = { home_dir = home_dir }
require("logic/engine")(shared)
require("logic/creations")(shared)
require("logic/draw")(shared)
