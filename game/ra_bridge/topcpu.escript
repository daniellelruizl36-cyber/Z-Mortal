#!/usr/bin/env escript
%%! -sname dbg2
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  Pids = rpc:call(T, 'Elixir.Process', list, []),
  R1 = maps:from_list([{P, Reds} || P <- Pids, {reductions, Reds} <- [rpc:call(T, 'Elixir.Process', info, [P, reductions])]]),
  timer:sleep(2000),
  Deltas = lists:sort(fun({_, A}, {_, B}) -> A > B end,
    lists:filtermap(fun(P) ->
      case rpc:call(T, 'Elixir.Process', info, [P, reductions]) of
        {reductions, R2} -> {true, {P, R2 - maps:get(P, R1, 0)}};
        _ -> false
      end
    end, Pids)),
  io:format("top reduction deltas:~n", []),
  lists:foreach(fun({P, D}) ->
    Init = rpc:call(T, 'Elixir.Process', info, [P, initial_call]),
    io:format("~p delta=~p init=~p~n", [P, D, Init])
  end, lists:sublist(Deltas, 6)).
