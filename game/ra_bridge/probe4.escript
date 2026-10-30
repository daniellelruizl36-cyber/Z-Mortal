#!/usr/bin/env escript
%%! -sname dbg10 -setcookie CREXDVYCDLVLLMDZZCTA
main(_) ->
  T = 'riichi_advanced@NagasakiSoyo',
  Ps = rpc:call(T, 'Elixir.Process', list, []),
  lists:foreach(fun(P) ->
    case rpc:call(T, 'Elixir.Process', info, [P, [dictionary]]) of
      undefined -> ok;
      I when is_list(I) ->
        D = proplists:get_value(dictionary, I, []),
        IC = proplists:get_value('$initial_call', D, none),
        S = lists:flatten(io_lib:format("~p", [IC])),
        case re:run(S, "GameLive|RoomLive") of
          {match, _} ->
            io:format("~s pid=~p~n", [S, P]);
          _ -> ok
        end;
      _ -> ok
    end
  end, Ps),
  ok.
