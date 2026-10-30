#!/usr/bin/env escript
%%! -sname dbg7
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  R = rpc:call(T, 'Elixir.RiichiAdvanced.R4Demo', default_mods, [<<"zhuyi_guangzhou">>]),
  io:format("default_mods count=~p~n", [length(R)]),
  R2 = rpc:call(T, 'Elixir.RiichiAdvanced.R4Demo', 'paused?', []),
  io:format("paused=~p~n", [R2]).
