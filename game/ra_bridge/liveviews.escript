#!/usr/bin/env escript
%%! -sname dbg5
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  Src = "
    for pid <- Process.list() do
      info = Process.info(pid, [:dictionary])
      dict = info[:dictionary] || []
      view = dict[:\"$initial_call\"]
      mod = if is_tuple(view), do: elem(view, 0), else: nil
      if mod != nil and to_string(mod) =~ \"Live\" do
        IO.inspect({pid, view})
      end
    end
  ",
  R = rpc:call(T, 'Elixir.Code', eval_string, [Src]),
  io:format("eval=~p~n", [R]).
