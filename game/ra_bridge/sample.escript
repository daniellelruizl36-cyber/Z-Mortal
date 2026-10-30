#!/usr/bin/env escript
%%! -sname dbg3
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  [{P1, _}] = rpc:call(T, 'Elixir.RiichiAdvanced.Utils', registry_lookup,
                       [<<"game_state">>, <<"zhuyi_guangzhou">>, <<"R4DEMO">>]),
  Pids = [P1],
  lists:foreach(fun(_) ->
    lists:foreach(fun(P) ->
      CF = rpc:call(T, 'Elixir.Process', info, [P, current_function]),
      io:format("~p~n", [CF])
    end, Pids)
  end, lists:seq(1, 15)).
