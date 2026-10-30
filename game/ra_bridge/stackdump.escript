#!/usr/bin/env escript
%%! -sname dbg
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  R = rpc:call(T, 'Elixir.RiichiAdvanced.Utils', registry_lookup,
               [<<"game_state">>, <<"zhuyi_guangzhou">>, <<"R4DEMO">>]),
  io:format("lookup=~p~n", [R]),
  [{P, _}] = R,
  {current_stacktrace, ST} = rpc:call(T, 'Elixir.Process', info, [P, current_stacktrace]),
  io:format("stacktrace:~n~p~n", [ST]),
  {status, Status} = rpc:call(T, 'Elixir.Process', info, [P, status]),
  io:format("status=~p~n", [Status]).
