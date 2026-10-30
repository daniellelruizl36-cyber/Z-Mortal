.after_initialization.actions += [["add_rule", "Rules", "No Double Yakuman", "Although yakuman can still stack, all yakuman are worth single yakuman each. For example, daisangen tsuuiisou is still double yakuman, but suuankou tanki is only worth a single yakuman."]]
|
# only zero the numeric component, preserving the unit (e.g. [2, "★"] -> [1, "★"])
# so yakuman whose value is an array are not demoted to regular han
if has("yakuman") then .yakuman |= map(.value |= (if (. | type) == "array" then .[0] = 1 else 1 end)) else . end
|
if has("meta_yakuman") then .meta_yakuman |= map(.value |= (if (. | type) == "array" then .[0] = 1 else 1 end)) else . end
