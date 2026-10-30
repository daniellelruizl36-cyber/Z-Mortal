defmodule RiichiAdvanced.GameState.MjaiBridge do
  @moduledoc """
  Appends mjai-format events to state.mjai_events for the R4 bridge bot.
  Hooked at the exact decision points in actions.ex so no retroactive
  modification (unlike Log.add_call) is needed.
  """
  alias RiichiAdvanced.GameState.Log, as: Log
  alias RiichiAdvanced.Utils, as: Utils

  @honor_to_mjai %{"1z" => "E", "2z" => "S", "3z" => "W", "4z" => "N", "5z" => "P", "6z" => "F", "7z" => "C"}
  @ra_to_mjai Map.merge(@honor_to_mjai, %{"0m" => "5mr", "0p" => "5pr", "0s" => "5sr"})

  def t(tile) do
    s = tile |> Utils.strip_attrs() |> to_string()
    Map.get(@ra_to_mjai, s, s)
  end

  defp append(state, event), do: Map.update!(state, :mjai_events, &(&1 ++ [event]))

  def draw(state, seat, tile) do
    append(state, %{"type" => "tsumo", "actor" => Log.to_seat(seat), "pai" => t(tile)})
  end

  def discard(state, seat, tile, tsumogiri, riichi) do
    actor = Log.to_seat(seat)
    state = if riichi, do: append(state, %{"type" => "reach", "actor" => actor}), else: state
    state = append(state, %{"type" => "dahai", "actor" => actor, "pai" => t(tile), "tsumogiri" => tsumogiri})
    if riichi, do: append(state, %{"type" => "reach_accepted", "actor" => actor}), else: state
  end

  def dora(state, tile) do
    append(state, %{"type" => "dora", "dora_marker" => t(tile)})
  end

  def call(state, seat, call_name, call_choice, called_tile) do
    actor = Log.to_seat(seat)
    consumed = Enum.map(call_choice, &t/1)
    event = case call_name do
      "chii" -> %{"type" => "chi", "actor" => actor, "target" => Log.to_seat(state.turn), "pai" => t(called_tile), "consumed" => consumed}
      "pon" -> %{"type" => "pon", "actor" => actor, "target" => Log.to_seat(state.turn), "pai" => t(called_tile), "consumed" => consumed}
      "daiminkan" -> %{"type" => "daiminkan", "actor" => actor, "target" => Log.to_seat(state.turn), "pai" => t(called_tile), "consumed" => consumed}
      "ankan" -> %{"type" => "ankan", "actor" => actor, "consumed" => consumed}
      "kakan" -> %{"type" => "kakan", "actor" => actor, "pai" => t(called_tile), "consumed" => consumed}
      _ -> nil
    end
    if event == nil, do: state, else: append(state, event)
  end
end
