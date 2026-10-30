import Config

# config/runtime.exs is executed for all environments, including
# during releases. It is executed after compilation and before the
# system starts, so it is typically used to load production configuration
# and secrets from environment variables or elsewhere. Do not define
# any compile-time configuration in here, as it won't be applied.
# The block below contains prod specific runtime configuration.

if config_env() == :prod do
  # Local packaged-release overrides (Z-Mortal fork):
  # prod.exs assumes `mix phx.server` from a repo checkout (watchers,
  # toplevel priv override, esbuild). A real release must not watch/reload,
  # and must serve priv from inside the release.
  config :riichi_advanced, RiichiAdvancedWeb.Endpoint,
    watchers: [],
    code_reloader: false,
    https: nil,
    check_origin: false,
    http: [
      ip: {0, 0, 0, 0},
      port: String.to_integer(System.get_env("HTTP_PORT") || "4002"),
      thousand_island_options: [transport_options: [reuseport: true]]
    ]

  config :riichi_advanced, :priv_dir, to_string(:code.priv_dir(:riichi_advanced))
end
