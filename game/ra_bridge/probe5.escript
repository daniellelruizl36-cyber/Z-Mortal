#!/usr/bin/env escript
%%! -sname dbg11 -setcookie CREXDVYCDLVLLMDZZCTA
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
        case re:run(S, "GameLive") of
          {match, _} ->
            St = rpc:call(T, sys, get_state, [P, 3000]),
            Socket = rpc:call(T, 'Elixir.Map', get, [St, socket, #{}]),
            Assigns = rpc:call(T, 'Elixir.Map', get, [Socket, assigns, #{}]),
            io:format("GL session=~p room=~p transport=~p~n",
              [rpc:call(T, 'Elixir.Map', get, [Assigns, session_id, none]),
               rpc:call(T, 'Elixir.Map', get, [Assigns, room_code, none]),
               rpc:call(T, 'Elixir.Map', get, [Socket, 'connected?', none])]);
          _ -> ok
        end;
      _ -> ok
    end
  end, Ps),
  ok.
