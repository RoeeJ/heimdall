defmodule Heimdall.Application do
  # See https://hexdocs.pm/elixir/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Ecto.Migrator,
       repos: Application.fetch_env!(:heimdall, :ecto_repos),
       skip: System.get_env("SKIP_MIGRATIONS") == "true"},
      HeimdallWeb.Telemetry,
      Heimdall.Repo,
      {DNSCluster, query: Application.get_env(:heimdall, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: Heimdall.PubSub},
      {Finch, name: Heimdall.Finch},
      {Cachex, name: :dns_cache, stats: true, transactions: true},
      {Heimdall.Servers.UDPServer, port: Application.get_env(:heimdall, :dns_port)},
      {Heimdall.Servers.TCPServer, port: Application.get_env(:heimdall, :dns_port)},
      {Heimdall.Servers.Limiter, name: Heimdall.Servers.Limiter},
      {Heimdall.Servers.Blocker, name: Heimdall.Servers.Blocker},
      {Heimdall.Servers.Tracker, name: Heimdall.Servers.Tracker},
      HeimdallWeb.Endpoint
    ]

    # See https://hexdocs.pm/elixir/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: Heimdall.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    HeimdallWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
