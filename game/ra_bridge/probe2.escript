#!/usr/bin/env escript
%%! -sname dbg8 -setcookie CREXDVYCDLVLLMDZZCTA
main(_) ->
  T = 'riichi_advanced@NagasakiSoyo',
  Ps = rpc:call(T, 'Elixir.Process', list, []),
  lists:foreach(fun(P) ->
    case rpc:call(T, 'Elixir.Process', info, [P, [dictionary]]) of
      undefined -> ok;
      I ->
        D = proplists:get_value(dictionary, I, []),
        IC = proplists:get_value('$initial_call', D, none),
        S = lists:flatten(io_lib:format("~p", [IC])),
        case re:run(S, "R4Player|GameState|RoomState") of
          {match, _} ->
            St = rpc:call(T, sys, get_state, [P, 3000]),
            StS = lists:flatten(io_lib:format("~0p", [St])),
            Short = if length(StS) > 700 -> lists:sublist(StS, 700); true -> StS end,
            io:format("== ~s~n~s~n~n", [S, Short]);
          _ -> ok
        end
    end
  end, Ps),
  ok.
