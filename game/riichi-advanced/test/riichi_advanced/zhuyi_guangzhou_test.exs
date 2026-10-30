defmodule RiichiAdvanced.ZhuyiGuangzhouTest do
  use ExUnit.Case, async: true
  alias RiichiAdvanced.TestUtils, as: TestUtils

  # closed dealer tsumo, 1 aka in hand: aka shuugi 1 chip per player
  test "guangzhou - closed tsumo with 1 aka" do
    TestUtils.test_yaku_advanced("zhuyi_guangzhou", [], [], """
    {
      "starting_hand": {
        "east": ["2p", "3p", "4p", "7p", "8p", "9p", "0s", "6s", "7s", "5m", "7m", "4z", "4z"],
        "south": ["1s", "2s", "3p", "2p", "3p", "4p", "3s", "4s", "6s", "1z", "1z", "1z", "4z"],
        "west": ["1p", "2p", "7p", "2p", "6p", "8p", "3s", "6s", "9s", "5z", "2z", "3z", "4z"],
        "north": ["9m", "9m", "3p", "6p", "7p", "2s", "5s", "8s", "1z", "2z", "3z", "4z", "5z"]
      },
      "starting_draws": ["6z", "6z", "7z", "8z", "6m"]
    }
    """, [
      %{"type" => "discard", "tile" => "6z", "player" => 0, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "6z", "player" => 1, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "7z", "player" => 2, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "8z", "player" => 3, "tsumogiri" => true},
      %{"type" => "buttons_pressed", "buttons" => [%{"button" => "tsumo"}, nil, nil, nil]}
    ], %{
      east: %{
        yaku: [{"Aka", [1, "Han", 1, "⛀"]}, {"Tsumo", [1, "Han"]}]
      }
    }, %{delta_scores: [3000, -1000, -1000, -1000], shuugi: [3, -1, -1, -1]})
  end

  # all three aka (0m 0p 0s), closed dealer tsumo: All Star 5 chips replaces individual aka chips
  test "guangzhou - all star tsumo" do
    TestUtils.test_yaku_advanced("zhuyi_guangzhou", [], [], """
    {
      "starting_hand": {
        "east": ["4m", "0m", "6m", "3p", "4p", "0p", "7p", "8p", "0s", "6s", "7s", "5z", "5z"],
        "south": ["1s", "2s", "3p", "2p", "3p", "4p", "3s", "4s", "6s", "1z", "1z", "1z", "4z"],
        "west": ["1p", "2p", "7p", "2p", "6p", "8p", "3s", "6s", "9s", "5z", "2z", "3z", "4z"],
        "north": ["9m", "9m", "3p", "6p", "7p", "2s", "5s", "8s", "1z", "2z", "3z", "4z", "5z"]
      },
      "starting_draws": ["6z", "6z", "7z", "8z", "9p"]
    }
    """, [
      %{"type" => "discard", "tile" => "6z", "player" => 0, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "6z", "player" => 1, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "7z", "player" => 2, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "8z", "player" => 3, "tsumogiri" => true},
      %{"type" => "buttons_pressed", "buttons" => [%{"button" => "tsumo"}, nil, nil, nil]}
    ], %{
      east: %{
        yaku: [{"Aka", [3, "Han"]}, {"All Star", [5, "⛀"]}, {"Tsumo", [1, "Han"]}]
      }
    }, %{delta_scores: [12000, -4000, -4000, -4000], shuugi: [15, -5, -5, -5]})
  end

  # kokushi musou dealer tsumo: yakuman shuugi 5 per player
  test "guangzhou - kokushi tsumo shuugi" do
    TestUtils.test_yaku_advanced("zhuyi_guangzhou", [], [], """
    {
      "starting_hand": {
        "east": ["1m", "1m", "9m", "1p", "9p", "1s", "9s", "1z", "2z", "3z", "4z", "5z", "6z"],
        "south": ["1s", "2s", "3p", "2p", "3p", "4p", "3s", "4s", "6s", "1z", "1z", "1z", "4z"],
        "west": ["1p", "2p", "7p", "2p", "6p", "8p", "3s", "6s", "9s", "5z", "2z", "3z", "4z"],
        "north": ["9m", "3m", "3p", "6p", "7p", "2s", "5s", "8s", "1z", "2z", "3z", "4z", "5z"]
      },
      "starting_draws": ["6z", "6z", "7z", "8z", "7z"]
    }
    """, [
      %{"type" => "discard", "tile" => "6z", "player" => 0, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "6z", "player" => 1, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "7z", "player" => 2, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "8z", "player" => 3, "tsumogiri" => true},
      %{"type" => "buttons_pressed", "buttons" => [%{"button" => "tsumo"}, nil, nil, nil]}
    ], %{}, %{delta_scores: [48000, -16000, -16000, -16000], shuugi: [15, -5, -5, -5]})
  end

  # open ron with 1 aka: discarder pays the aka shuugi chip
  test "guangzhou - closed ron with 1 aka" do
    TestUtils.test_yaku_advanced("zhuyi_guangzhou", [], [], """
    {
      "starting_hand": {
        "east": ["4s", "0s", "6s", "2p", "3p", "4p", "5m", "6m", "7m", "2s", "3s", "8s", "8s"],
        "south": ["1s", "2s", "3p", "2p", "3p", "4p", "3s", "4s", "6s", "1z", "1z", "1z", "4z"],
        "west": ["1p", "2p", "7p", "2p", "6p", "8p", "3s", "6s", "9s", "5z", "2z", "3z", "4z"],
        "north": ["9m", "3m", "3p", "6p", "7p", "2s", "5s", "8s", "1z", "2z", "3z", "4z", "5z"]
      },
      "starting_draws": ["6z", "4s"]
    }
    """, [
      %{"type" => "discard", "tile" => "6z", "player" => 0, "tsumogiri" => true},
      %{"type" => "discard", "tile" => "4s", "player" => 1, "tsumogiri" => true},
      %{"type" => "buttons_pressed", "buttons" => [%{"button" => "ron"}, nil, nil, nil]}
    ], %{
      east: %{
        yaku: [{"Aka", [1, "Han", 1, "⛀"]}, {"Pinfu", [1, "Han"]}, {"Tanyao", [1, "Han"]}]
      }
    }, %{delta_scores: [5800, -5800, 0, 0], shuugi: [1, -1, 0, 0]})
  end
end
