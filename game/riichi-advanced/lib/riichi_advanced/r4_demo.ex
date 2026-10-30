defmodule RiichiAdvanced.R4Demo do
  @moduledoc """
  Demo mode (env R4_DEMO=1): start a zhuyi_guangzhou game room "R4DEMO"
  where all four seats are R4 bots, and keep it alive/restartable for
  spectating at /game/zhuyi_guangzhou/R4DEMO.
  """
  alias RiichiAdvanced.Utils, as: Utils

  @ruleset "zhuyi_guangzhou"
  @room "R4DEMO"

  def enabled?, do: System.get_env("R4_DEMO") == "1"

  def paused? do
    System.get_env("R4_DEMO_PAUSE") == "1" or
      Application.get_env(:riichi_advanced, :r4_demo_pause, false)
  end

  def pause do
    Application.put_env(:riichi_advanced, :r4_demo_pause, true)
    IO.puts("R4DEMO: paused (human play mode)")
  end

  def resume do
    Application.put_env(:riichi_advanced, :r4_demo_pause, false)
    :timer.apply_after(1_000, __MODULE__, :ensure_game, [false])
    IO.puts("R4DEMO: resumed")
  end

  def maybe_start do
    if enabled?() do
      Application.put_env(:riichi_advanced, :ai_module, RiichiAdvanced.R4Player)
      :timer.apply_after(3_000, __MODULE__, :ensure_game, [false])
    end
    :ok
  end

  # filled? tracks whether we've already cast fill for the current game process
  def ensure_game(filled?) do
    if enabled?() and not paused?() do
      case Utils.registry_lookup("game_state", @ruleset, @room) do
        [] ->
          start_game()
          :timer.apply_after(2_000, __MODULE__, :ensure_game, [false])
        [{game_state, _}] ->
          filled? = if not filled? do
            GenServer.cast(game_state, {:fill_empty_seats_with_ai, false})
            Process.sleep(1_000)
            GenServer.cast(game_state, {:initialize_game, nil})
            true
          else filled? end
          :timer.apply_after(5_000, __MODULE__, :ensure_game, [filled?])
      end
    end
    :ok
  end

  defp start_game do
    # GameState's init does NOT apply default mods itself (the lobby normally
    # does); passing mods: [] would compile the ruleset with zero yaku/mods.
    default_mods = load_default_mods()
    args = [
      room_code: @room,
      ruleset: @ruleset,
      mods: default_mods,
      config: "",
      private: true,
      reserved_seats: %{},
      init_actions: [],
      name: Utils.via_registry("game", @ruleset, @room)
    ]
    game_spec = Supervisor.child_spec(%{
      id: {RiichiAdvanced.GameSupervisor, @ruleset, @room},
      start: {RiichiAdvanced.GameSupervisor, :start_link, [args]}
    }, restart: :temporary)
    case DynamicSupervisor.start_child(RiichiAdvanced.GameSessionSupervisor, game_spec) do
      {:ok, _pid} -> IO.puts("R4DEMO: started game session (mods=#{length(default_mods)})")
      {:error, err} -> IO.puts("R4DEMO: start error #{inspect(err)}")
    end
  end

  def default_mods(ruleset) do
    alias RiichiAdvanced.GameState.Rules, as: Rules
    alias RiichiAdvanced.ModLoader.ModState, as: ModState
    ruleset_json = ModState.load_ruleset(ruleset) |> ModState.extract_json()
    case Rules.load_rules(ruleset_json, ruleset) do
      {:ok, base_rules_ref} -> Rules.get(base_rules_ref, "default_mods", [])
      {:error, msg} ->
        IO.puts("R4DEMO: failed to load base ruleset: #{msg}")
        []
    end
  end

  defp load_default_mods, do: default_mods(@ruleset)
end
