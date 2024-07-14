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
      # Start the Finch HTTP client for sending emails
      {Finch, name: Heimdall.Finch},
      # Start a worker by calling: Heimdall.Worker.start_link(arg)
      # {Heimdall.Worker, arg},
      # Start to serve requests, typically the last entry
      HeimdallWeb.Endpoint,
      Heimdall.DNS.Manager,
      Heimdall.DNS.Resolver,
      {Heimdall.DNS.Server, port: Application.get_env(:heimdall, :dns_port) || 1053},
      {Cachex, name: :dns_cache, stats: true, transactions: true}
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
