#!/usr/bin/env escript
%%! -sname dbg6
main(_) ->
  T = 'r4demo@NagasakiSoyo',
  Src = "
    [{gs, _}] = RiichiAdvanced.Utils.registry_lookup(\"game_state\", \"zhuyi_guangzhou\", \"R4DEMO\")
    st = GenServer.call(gs, :get_state, :infinity)
    for seat <- [:east, :south, :west, :north] do
      p = Map.get(st, seat)
      alive = is_pid(p) and Process.alive?(p)
      init = if alive, do: Process.info(p)[:dictionary][:\"$initial_call\"], else: nil
      mod = if is_tuple(init), do: elem(init, 0), else: init
      IO.inspect({seat, p, alive, mod, st.players[seat].nickname})
    end
    IO.inspect({:kyoku, st.kyoku, :wall, length(st.wall), :turn, st.turn, :awaiting, st.awaiting_discard})
  ",
  R = rpc:call(T, 'Elixir.Code', eval_string, [Src]),
  io:format("eval=~p~n", [R]).
