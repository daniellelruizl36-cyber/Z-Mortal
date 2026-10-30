#!/usr/bin/env escript
%%! -sname dbg2 -setcookie CREXDVYCDLVLLMDZZCTA
main(_) ->
  T = 'riichi_advanced@NagasakiSoyo',
  R1 = rpc:call(T, 'Elixir.RiichiAdvanced.R4Demo', 'enabled?', []),
  R2 = rpc:call(T, 'Elixir.RiichiAdvanced.R4Demo', 'paused?', []),
  io:format("enabled=~p paused=~p~n", [R1, R2]),
  R3 = rpc:call(T, 'Elixir.Registry', select, ['Elixir.RiichiAdvanced.Registry', [{{{'$1','$2','$3'},'$4', '_'},[],['{{$1,$2,$3}}']}], 100]),
  io:format("registry=~p~n", [R3]).
