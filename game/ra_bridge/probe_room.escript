#!/usr/bin/env escript
%%! -sname dbg7 -setcookie CREXDVYCDLVLLMDZZCTA
main(_) ->
  T = 'riichi_advanced@NagasakiSoyo',
  io:format('self=~p cookie=~p~n',[node(), erlang:get_cookie()]), Rs = rpc:call(T, 'Elixir.Process', registered, []),
  io:format("registered=~p~n", [Rs]),
  Ps = rpc:call(T, 'Elixir.Process', list, []),
  lists:foreach(fun(P) ->
    I = rpc:call(T, 'Elixir.Process', info, [P, [dictionary]]),
    D = proplists:get_value(dictionary, I, []),
    IC = proplists:get_value('$initial_call', D, none),
    ICStr = io_lib:format("~p", [IC]),
    S = lists:flatten(ICStr),
    case re:run(S, "Room|Game|R4|Lobby") of
      {match, _} ->
        ST = rpc:call(T, 'Elixir.Process', info, [P, [status]]),
        io:format("~p ~s status=~p~n", [P, S, proplists:get_value(status, ST)]);
      _ -> ok
    end
  end, Ps),
  ok.
