#!/usr/bin/env escript
%%! -sname dbg9
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  R = rpc:call(T, 'Elixir.RiichiAdvanced.Cache', delete_all, []),
  io:format("cleared: ~p~n", [R]),
  Src = "
    mods = RiichiAdvanced.R4Demo.default_mods(\"zhuyi_guangzhou\")
    shuugi = Enum.find(mods, fn m -> is_map(m) and m[\"name\"] == \"lib/shuugi\" end)
    uma = Enum.find(mods, fn m -> is_map(m) and m[\"name\"] == \"uma\" end)
    oka = Enum.find(mods, fn m -> is_map(m) and m[\"name\"] == \"oka\" end)
    IO.inspect(shuugi[\"config\"], label: \"shuugi\")
    IO.inspect(uma[\"config\"], label: \"uma\")
    IO.inspect(oka, label: \"oka\")
  ",
  R2 = rpc:call(T, 'Elixir.Code', eval_string, [Src]),
  io:format("eval=~p~n", [R2]).
