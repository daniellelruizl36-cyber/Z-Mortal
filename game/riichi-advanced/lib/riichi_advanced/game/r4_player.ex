defmodule RiichiAdvanced.R4Player do
  @moduledoc """
  R4 model bridge bot. Mirrors the Riichi Advanced game into libriichi's
  mjai.Bot (served by ra_bridge/r4_service.py over local HTTP) and translates
  the bot's mjai actions back into GameState casts.

  The authoritative event source is state.mjai_events (appended inline by
  RiichiAdvanced.GameState.MjaiBridge at draw/discard/call/dora decision
  points), so no retroactive modification has to be undone.
  """
  use GenServer
  alias RiichiAdvanced.GameState.Log, as: Log
  alias RiichiAdvanced.GameState.Rules, as: Rules
  alias RiichiAdvanced.Utils, as: Utils

  @service_url "http://127.0.0.1:8471"
  @http_timeout 60_000
  @discard_delay 700
  @button_delay 250
  @call_delay 400

  @honor_to_mjai %{"1z" => "E", "2z" => "S", "3z" => "W", "4z" => "N", "5z" => "P", "6z" => "F", "7z" => "C"}
  @ra_to_mjai Map.merge(@honor_to_mjai, %{"0m" => "5mr", "0p" => "5pr", "0s" => "5sr"})
  @mjai_to_ra Map.new(@ra_to_mjai, fn {k, v} -> {v, String.to_atom(k)} end)

  def start_link(init_state) do
    GenServer.start_link(__MODULE__, init_state, name: init_state[:name])
  end

  @impl true
  def init(state) do
    state = state
    |> Map.put(:initialized, false)
    |> Map.put(:seat_ix, Log.to_seat(state.seat))
    |> Map.put(:mjai_len, 0)
    |> Map.put(:kyoku_key, nil)
    |> Map.put(:pending_riichi, false)
    |> Map.put(:riichi_mode, :none)
    |> Map.put(:pending_action, nil)
    |> Map.put(:sent_start_game, false)
    |> Map.put(:game_id, nil)
    |> Map.put(:deciding, false)
    :timer.apply_after(500, Kernel, :send, [self(), :initialize])
    {:ok, state}
  end

  @impl true
  def terminate(reason, state) do
    IO.puts("R4Player(#{state.seat}) TERMINATE reason=#{inspect(reason)}")
    :ok
  end

  # ---------- tile conversion ----------

  def ra_to_mjai(tile) do
    s = tile |> Utils.strip_attrs() |> to_string()
    Map.get(@ra_to_mjai, s, s)
  end

  def mjai_to_ra(pai), do: Map.get(@mjai_to_ra, pai, String.to_atom(pai))

  # ---------- HTTP ----------

  defp post(payload) do
    body = Jason.encode!(payload)
    req = {String.to_charlist(@service_url), [], ~c"application/json", body}
    case :httpc.request(:post, req, [timeout: @http_timeout], []) do
      {:ok, {{_, 200, _}, _, resp}} -> {:ok, Jason.decode!(resp)}
      {:ok, {{_, code, _}, _, resp}} -> {:error, "HTTP #{code}: #{resp}"}
      {:error, reason} -> {:error, inspect(reason)}
    end
  end

  defp feed_event(game_id, seat_ix, ev) do
    case post(%{kind: "event", game_id: game_id, seat: seat_ix, line: ev}) do
      {:ok, %{"action" => action}} -> action
      {:ok, _} -> nil
      {:error, err} ->
        IO.puts("R4Player(#{game_id}/#{seat_ix}) feed error on #{inspect(ev)}: #{err}")
        nil
    end
  end

  defp feed_events(game_id, seat_ix, events) do
    Enum.reduce(events, nil, fn ev, acc ->
      case feed_event(game_id, seat_ix, ev) do
        nil -> acc
        action -> action
      end
    end)
  end

  # ---------- mirror sync ----------

  # Feed new mjai_events to the mirror, skipping my own riichi triple when it
  # was already fed at synthesis time. Returns {state, action_or_nil}.
  defp sync_events(state, gs) do
    new = Enum.drop(gs.mjai_events, state.mjai_len)
    IO.puts("R4Player(#{state.seat}) sync +#{length(new)} [#{Enum.map_join(new, ",", & &1["type"])}] len #{state.mjai_len}->#{length(gs.mjai_events)}")
    {state, events, _skip} = Enum.reduce(new, {state, [], false}, fn ev, {st, evs, skip} ->
      mine? = ev["actor"] == st.seat_ix
      cond do
        # inside my own riichi triple: skip until reach_accepted
        skip and mine? and ev["type"] in ["dahai", "reach_accepted"] ->
          st = if ev["type"] == "reach_accepted", do: %{st | pending_riichi: false, riichi_mode: :none}, else: st
          {st, evs, false}
        # my synthesized riichi just showed up in the stream: skip its reach
        st.pending_riichi and mine? and ev["type"] == "reach" ->
          {st, evs, true}
        # my plain discard while a riichi press was pending: press must have
        # failed; clear the stale flag and feed normally
        st.pending_riichi and mine? and ev["type"] == "dahai" ->
          {%{st | pending_riichi: false}, evs ++ [ev], skip}
        true ->
          {st, evs ++ [ev], skip}
      end
    end)
    state = %{state | mjai_len: length(gs.mjai_events)}
    action = if events == [], do: nil, else: feed_events(state.game_id, state.seat_ix, events)
    {state, action}
  end

  defp start_kyoku_event(gs) do
    seats = gs.available_seats
    bakaze = RiichiAdvanced.Riichi.get_round_wind(gs.kyoku, length(seats)) |> to_string() |> String.first() |> String.upcase()
    oya = RiichiAdvanced.Riichi.get_east_player_seat(gs.kyoku, seats) |> Log.to_seat()
    kyoku_no = Integer.mod(gs.kyoku, length(seats)) + 1
    riichi_value = Rules.get(gs.rules_ref, "score_calculation", %{}) |> Map.get("riichi_value", 1000)
    kyotaku = Integer.floor_div(gs.pot, riichi_value)
    dora_marker = Enum.at(gs.dead_wall, -6) |> ra_to_mjai()
    %{
      "type" => "start_kyoku",
      "bakaze" => bakaze,
      "dora_marker" => dora_marker,
      "kyoku" => kyoku_no,
      "honba" => gs.honba,
      "kyotaku" => kyotaku,
      "oya" => oya,
      "scores" => Enum.map(seats, fn s -> gs.players[s].score end),
      "tehais" => Enum.map(seats, fn s -> Enum.map(gs.haipai[s], &ra_to_mjai/1) end)
    }
  end

  # ---------- RA action helpers ----------

  defp playables(state) do
    player = state.player
    (Enum.with_index(player.hand) ++ Enum.with_index(player.draw, length(player.hand)))
    |> Enum.filter(fn {tile, _i} ->
      GenServer.call(state.game_state, {:is_playable, state.seat, tile}, :infinity)
    end)
  end

  defp cast_play_tile(state, pai, tsumogiri) do
    target = mjai_to_ra(pai)
    tiles = state.player.hand ++ state.player.draw
    playables = playables(state)
    indexed = Enum.with_index(tiles)
    candidates = if tsumogiri, do: Enum.reverse(indexed) ++ indexed, else: indexed
    exact = Enum.find(candidates, fn {tile, i} ->
      Utils.same_tile(tile, target) and Enum.any?(playables, fn {_t, j} -> j == i end)
    end)
    {_tile, index} = exact || List.last(playables) || List.last(indexed)
    GenServer.cast(state.game_state, {:play_tile, state.seat, index})
  end

  defp fallback_discard(state) do
    case playables(state) do
      [] -> IO.puts("R4Player(#{state.seat}): no playable tiles for fallback discard")
      playables ->
        {_, index} = List.last(playables)
        GenServer.cast(state.game_state, {:play_tile, state.seat, index})
    end
  end

  defp press_skip(state, turn) do
    player = state.player
    if "skip" in player.buttons do
      if state.seat == turn and Enum.empty?(player.deferred_context_actions) do
        GenServer.cast(state.game_state, {:ai_ignore_buttons, state.seat})
      else
        GenServer.cast(state.game_state, {:press_button, state.seat, "skip"})
      end
    else
      GenServer.cast(state.game_state, {:ai_ignore_buttons, state.seat})
    end
  end

  defp same_tiles?(choice, consumed) do
    stripped = Enum.map(choice, &Utils.strip_attrs/1)
    length(stripped) == length(consumed) and
      Enum.all?(Enum.zip(stripped, consumed), fn {a, b} -> Utils.same_tile(a, b) end)
  end

  defp press_kan(state, kind, action) do
    consumed = Enum.map(action["consumed"] || [], &mjai_to_ra/1)
    choice =
      case state.player.button_choices[kind] do
        {:call, call_choices} ->
          all = Enum.flat_map(call_choices, fn {_called, choices} -> choices end)
          Enum.find(all, &same_tiles?(&1, consumed)) ||
            Enum.find(all, fn c -> length(c) == length(consumed) end) ||
            List.first(all)
        _ -> nil
      end
    if choice != nil do
      GenServer.cast(state.game_state, {:press_call_button, state.seat, choice, nil})
    else
      GenServer.cast(state.game_state, {:press_button, state.seat, kind})
    end
  end

  defp press_call(state, act, context) do
    player = state.player
    cb = player.call_buttons || %{}
    if cb == %{} and context != :call_buttons do
      press_skip(state, nil)
      state
    else
      target = mjai_to_ra(act["pai"])
      consumed = Enum.map(act["consumed"] || [], &mjai_to_ra/1)
      {called_key, choices} =
        Enum.find(cb, fn {tile, cs} -> cs != [] and Utils.same_tile(Utils.to_tile(tile), target) end) ||
        Enum.find(cb, fn {_tile, cs} -> cs != [] end) || {nil, []}
      choice = Enum.find(choices, &same_tiles?(&1, consumed)) || List.first(choices)
      Process.sleep(@call_delay)
      if choice != nil and called_key != nil do
        GenServer.cast(state.game_state, {:press_call_button, state.seat, choice, called_key})
      else
        GenServer.cast(state.game_state, {:cancel_call_buttons, state.seat})
      end
      state
    end
  end

  # ---------- mjai action handling ----------

  defp handle_action(state, action, context, turn) do
    player = state.player
    case action do
      %{"type" => "dahai", "pai" => pai, "tsumogiri" => tg} ->
        Process.sleep(@discard_delay)
        cast_play_tile(state, pai, tg)
        state
      %{"type" => "reach"} ->
        Process.sleep(@button_delay)
        fresh = GenServer.call(state.game_state, :get_state, :infinity)
        fresh_buttons = fresh.players[state.seat].buttons
        fp = fresh.players[state.seat]
        tenpai_defs = Rules.translate_match_definitions(fresh.rules_ref, ["tenpai_14"])
        tenpai = RiichiAdvanced.Match.match_hand(fp.hand ++ fp.draw, fp.calls, tenpai_defs, fp.tile_behavior)
        raw_def = Rules.get(fresh.rules_ref, "tenpai_14_definition")
        all_btns = Rules.get(fresh.rules_ref, "buttons", %{}) |> Map.keys()
        yakus = Rules.get(fresh.rules_ref, :yaku, [])
        IO.puts("R4Player(#{state.seat}) REACH_CHECK buttons=#{inspect(fresh_buttons)} turn=#{fresh.turn} tenpai14=#{tenpai} raw_def=#{raw_def != nil} btns=#{inspect(all_btns)} yaku_count=#{length(yakus)} hand=#{inspect(Enum.map(fp.hand ++ fp.draw, fn t -> t |> Utils.strip_attrs() |> to_string() end))} defs=#{length(tenpai_defs)}")
        if "riichi" in fresh_buttons do
          GenServer.cast(state.game_state, {:press_button, state.seat, "riichi"})
          %{state | pending_riichi: true}
        else
          fallback_discard(state)
          state
        end
      %{"type" => "hora"} ->
        Process.sleep(@button_delay)
        cond do
          "tsumo" in player.buttons -> GenServer.cast(state.game_state, {:press_button, state.seat, "tsumo"})
          "ron" in player.buttons -> GenServer.cast(state.game_state, {:press_button, state.seat, "ron"})
          "hu" in player.buttons -> GenServer.cast(state.game_state, {:press_button, state.seat, "hu"})
          "zimo" in player.buttons -> GenServer.cast(state.game_state, {:press_button, state.seat, "zimo"})
          true -> IO.puts("R4Player(#{state.seat}): hora but no win button in #{inspect(player.buttons)}")
        end
        state
      %{"type" => "ankan"} = act ->
        Process.sleep(@button_delay)
        if "ankan" in player.buttons do
          press_kan(state, "ankan", act)
        else
          IO.puts("R4Player(#{state.seat}): ankan not in buttons #{inspect(player.buttons)}")
          if context == :your_turn, do: fallback_discard(state)
        end
        state
      %{"type" => "kakan"} = act ->
        Process.sleep(@button_delay)
        if "kakan" in player.buttons do
          press_kan(state, "kakan", act)
        else
          if context == :your_turn, do: fallback_discard(state)
        end
        state
      %{"type" => "chi"} = act -> press_call(state, act, context)
      %{"type" => "pon"} = act -> press_call(state, act, context)
      %{"type" => "daiminkan"} = act -> press_call(state, act, context)
      %{"type" => "none"} ->
        case context do
          :call_buttons ->
            Process.sleep(@call_delay)
            GenServer.cast(state.game_state, {:cancel_call_buttons, state.seat})
          _ -> press_skip(state, turn)
        end
        state
      _ ->
        case context do
          :your_turn ->
            IO.puts("R4Player(#{state.seat}): fallback discard (action=#{inspect(action)})")
            Process.sleep(@discard_delay)
            fallback_discard(state)
          :buttons -> press_skip(state, turn)
          :call_buttons ->
            Process.sleep(@call_delay)
            GenServer.cast(state.game_state, {:cancel_call_buttons, state.seat})
        end
        state
    end
  end

  # Riichi is a marking button in RA (cancellable_riichi.jq): pressing it
  # opens a mark-1-hand-tile window and the deferred actions discard the
  # marked tile without ever sending us a your_turn. Ask the mirror for the
  # post-reach discard and mark exactly that tile.
  defp resolve_riichi_mark(state) do
    reach_ev = %{"type" => "reach", "actor" => state.seat_ix}
    case feed_event(state.game_id, state.seat_ix, reach_ev) do
      %{"type" => "dahai", "pai" => pai, "tsumogiri" => tg} = act ->
        feed_events(state.game_id, state.seat_ix, [
          Map.drop(act, ["meta"]),
          %{"type" => "reach_accepted", "actor" => state.seat_ix}
        ])
        Process.sleep(@discard_delay)
        tiles = state.player.hand ++ state.player.draw
        playables = state.player.cache.playable_indices || []
        target = mjai_to_ra(pai)
        indexed = Enum.with_index(tiles)
        candidates = if tg, do: Enum.reverse(indexed) ++ indexed, else: indexed
        match = Enum.find(candidates, fn {tile, i} ->
          Utils.same_tile(tile, target) and (playables == [] or i in playables)
        end)
        {_tile, index} = match ||
          (if playables != [] do
             ix = Enum.find(playables, &(&1 < length(tiles)))
             ix && Enum.at(indexed, ix)
           end) ||
          List.last(indexed)
        IO.puts("R4Player(#{state.seat}) RIICHI_MARK pai=#{pai} tsumogiri=#{tg} index=#{index} playables=#{inspect(playables)}")
        GenServer.cast(state.game_state, {:mark_tile, state.seat, state.seat, index, :hand})
      other ->
        IO.puts("R4Player(#{state.seat}): unexpected post-reach action #{inspect(other)}, clearing mark")
        GenServer.cast(state.game_state, {:clear_marked_objects, state.seat})
    end
    state
  end

  # After pressing the riichi button, feed a synthesized reach event so the
  # mirror produces the discard decision, then mirror reach+dahai+accepted.
  defp resolve_pending_riichi(state) do
    reach_ev = %{"type" => "reach", "actor" => state.seat_ix}
    case feed_event(state.game_id, state.seat_ix, reach_ev) do
      %{"type" => "dahai", "pai" => pai, "tsumogiri" => tg} = act ->
        feed_events(state.game_id, state.seat_ix, [
          Map.drop(act, ["meta"]),
          %{"type" => "reach_accepted", "actor" => state.seat_ix}
        ])
        Process.sleep(@discard_delay)
        cast_play_tile(state, pai, tg)
      other ->
        IO.puts("R4Player(#{state.seat}): unexpected post-reach action #{inspect(other)}, tsumogiri")
        feed_event(state.game_id, state.seat_ix, %{"type" => "reach_accepted", "actor" => state.seat_ix})
        Process.sleep(@discard_delay)
        fallback_discard(state)
    end
  end

  # Core decision path used by every notification.
  defp decide(state, context, turn) do
    GenServer.cast(state.game_state, {:ai_thinking, state.seat})
    gs = GenServer.call(state.game_state, :get_state, :infinity)
    {state, action} = sync_events(state, gs)
    IO.puts("R4Player(#{state.seat}) #{context}: #{if(action, do: action["type"], else: "nil")} (kyoku=#{gs.kyoku} wall=#{length(gs.wall)})")

    if state.pending_riichi and action == nil and state.riichi_mode != :marking do
      resolve_pending_riichi(state)
      state
    else
      cond do
        action == nil and state.pending_action != nil ->
          handle_action(%{state | pending_action: nil}, state.pending_action, context, turn)
        action != nil and context == :buttons and state.seat == turn and
            action["type"] == "dahai" ->
          # RA's buttons phase precedes the discard; save the mirror's
          # discard-phase answer for the your_turn that follows. The buttons
          # window must still be closed or the your_turn never arrives.
          if "skip" in state.player.buttons do
            GenServer.cast(state.game_state, {:press_button, state.seat, "skip"})
          end
          %{state | pending_action: action}
        true ->
          handle_action(state, action, context, turn)
      end
    end
  end

  # ---------- GenServer callbacks ----------

  @impl true
  def handle_info(:initialize, state) do
    gs = GenServer.call(state.game_state, :get_state, :infinity)
    cond do
      not is_map(gs.haipai) or map_size(gs.haipai) == 0 ->
        # round not dealt yet (we started before initialize_game); retry later
        :timer.apply_after(1_000, Kernel, :send, [self(), :initialize])
        {:noreply, state}
      state.initialized and state.kyoku_key == {gs.kyoku, gs.honba} ->
        # duplicate :initialize (RA sends one per round too); already synced
        {:noreply, state}
      true ->
        do_initialize(state, gs)
    end
  end

  defp do_initialize(state, gs) do
    game_id = if gs.ref == "" do gs.room_code else gs.ref end
    state = %{state | game_id: game_id}

    post(%{kind: "init", game_id: game_id})

    kyoku_key = {gs.kyoku, gs.honba}
    state =
      if state.kyoku_key != nil and state.kyoku_key != kyoku_key do
        feed_event(game_id, state.seat_ix, %{"type" => "end_kyoku"})
        %{state | mjai_len: 0, pending_riichi: false, riichi_mode: :none, pending_action: nil}
      else
        state
      end
    state = %{state | kyoku_key: kyoku_key}

    state = if not state.sent_start_game do
      feed_event(game_id, state.seat_ix, %{"type" => "start_game", "names" => ["R4", "R4", "R4", "R4"]})
      %{state | sent_start_game: true}
    else
      state
    end

    sk = start_kyoku_event(gs)
    feed_event(game_id, state.seat_ix, sk)
    backlog_action = feed_events(game_id, state.seat_ix, gs.mjai_events)
    IO.puts("R4Player(#{state.seat}) init kyoku=#{gs.kyoku} backlog=#{length(gs.mjai_events)} backlog_action=#{if(backlog_action, do: backlog_action["type"], else: "nil")}")
    state = %{state | mjai_len: length(gs.mjai_events)}
    state = if backlog_action != nil, do: %{state | pending_action: backlog_action}, else: state

    GenServer.cast(state.game_state, :notify_ai)
    {:noreply, %{state | initialized: true}}
  end

  @impl true
  def handle_info({:your_turn, %{player: player}}, state) do
    cond do
      not state.initialized ->
        :timer.apply_after(1000, Kernel, :send, [self(), {:your_turn, %{player: player}}])
        {:noreply, state}
      state.deciding ->
        {:noreply, state}
      true ->
        IO.puts("R4Player(#{state.seat}) YOUR_TURN_BUTTONS=#{inspect(player.buttons)}")
        state = %{state | player: player, deciding: true}
        {:noreply, %{decide(state, :your_turn, state.seat) | deciding: false}}
    end
  end

  @impl true
  def handle_info({:buttons, %{player: player, turn: turn}}, state) do
    cond do
      not state.initialized ->
        :timer.apply_after(1000, Kernel, :send, [self(), {:buttons, %{player: player, turn: turn}}])
        {:noreply, state}
      state.deciding ->
        {:noreply, state}
      true ->
        IO.puts("R4Player(#{state.seat}) BUTTONS=#{inspect(player.buttons)} turn=#{turn}")
        state = %{state | player: player, deciding: true}
        {:noreply, %{decide(state, :buttons, turn) | deciding: false}}
    end
  end

  @impl true
  def handle_info({:call_buttons, %{player: player}}, state) do
    cond do
      not state.initialized ->
        {:noreply, state}
      state.deciding ->
        {:noreply, state}
      true ->
        IO.puts("R4Player(#{state.seat}) CALL_BUTTONS=#{inspect(player.buttons)}")
        state = %{state | player: player, deciding: true}
        {:noreply, %{decide(state, :call_buttons, nil) | deciding: false}}
    end
  end

  @impl true
  def handle_info({:mark_tiles, %{player: player}}, state) do
    cond do
      not state.initialized ->
        {:noreply, state}
      state.deciding ->
        {:noreply, state}
      true ->
        state = %{state | player: player, deciding: true}
        state = case player.button_choices["riichi"] do
          {:mark, _spec, _pre, _post, _cancel} ->
            resolve_riichi_mark(%{state | riichi_mode: :marking})
          _ ->
            GenServer.cast(state.game_state, {:clear_marked_objects, state.seat})
            state
        end
        {:noreply, %{state | deciding: false}}
    end
  end

  @impl true
  def handle_info({:declare_yaku, %{player: player}}, state) do
    if state.initialized do
      GenServer.cast(state.game_state, {:declare_yaku, state.seat, ["Riichi"]})
      {:noreply, %{state | player: player}}
    else
      {:noreply, state}
    end
  end

  @impl true
  def handle_info(_msg, state), do: {:noreply, state}
end
