#!/usr/bin/env escript
%%! -sname dbg9 -setcookie CREXDVYCDLVLLMDZZCTA
dump(T, Re, Label) ->
  Ps = rpc:call(T, 'Elixir.Process', list, []),
  lists:foreach(fun(P) ->
    case rpc:call(T, 'Elixir.Process', info, [P, [dictionary]]) of
      undefined -> ok;
      I when is_list(I) ->
        D = proplists:get_value(dictionary, I, []),
        IC = proplists:get_value('$initial_call', D, none),
        S = lists:flatten(io_lib:format("~p", [IC])),
        case re:run(S, Re) of
          {match, _} ->
            St = rpc:call(T, sys, get_state, [P, 3000]),
            print(Label, St);
          _ -> ok
        end;
      _ -> ok
    end
  end, Ps).
print(r4p, St) ->
  io:format("R4P seat=~p deciding=~p pending=~p mjai_len=~p kyoku_key=~p~n",
    [maps:get(seat, St), maps:get(deciding, St),
     maps:get(pending_action, St) /= nil, maps:get(mjai_len, St),
     maps:get(kyoku_key, St)]);
print(gs, St) ->
  Turn = maps:get(turn, St, none),
  Kyoku = maps:get(kyoku, St, none),
  Wall = maps:get(wall, St, []),
  Ps2 = maps:get(players, St, #{}),
  Ready = maps:fold(fun(K, V, Acc) -> [{K, maps:get(ready, V, none)} | Acc] end, [], Ps2),
  io:format("GS turn=~p kyoku=~p wall_len=~p ready=~p~n", [Turn, Kyoku, length(Wall), Ready]).
main(_) ->
  T = 'riichi_advanced@NagasakiSoyo',
  dump(T, "R4Player", r4p),
  dump(T, "GameState", gs),
  ok.
