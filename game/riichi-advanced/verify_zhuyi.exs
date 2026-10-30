alias RiichiAdvanced.ModLoader.ModState
m = ModState.load_ruleset("zhuyi_guangzhou")
json0 = ModState.extract_json(m)
{:ok, ref} = RiichiAdvanced.GameState.Rules.load_rules(json0, "zhuyi_guangzhou")
default_mods = RiichiAdvanced.GameState.Rules.get(ref, "default_mods", [])
IO.puts("default_mods count: #{length(default_mods)}")
m2 = m |> ModState.prepend_mods(default_mods)
json = ModState.extract_json(m2)
File.write!("zhuyi_guangzhou_full.json", json)
IO.puts("full JSON length: #{String.length(json)}")
{:ok, parsed} = Jason.decode(json)
IO.inspect(Map.keys(parsed["functions"]) |> Enum.filter(&String.contains?(&1, "shuugi")), label: "shuugi functions")
IO.inspect(parsed["yaku_lists"], label: "yaku_lists")
for list <- parsed["yaku_lists"] || [] do
  entries = parsed[list] || []
  matched = Enum.filter(entries, fn e -> Enum.any?(e, fn x -> is_binary(x) and String.contains?(x, ["Aka", "All Star", "Rinshan", "Haitei"]) end) end)
  if matched != [], do: IO.inspect(matched, label: list)
end
IO.inspect(Enum.filter(parsed["after_win"]["actions"] || [], fn a -> Jason.encode!(a) |> String.contains?("shuugi") end), label: "after_win shuugi actions")
