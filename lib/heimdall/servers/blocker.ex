defmodule Heimdall.Servers.Blocker do
  @moduledoc """
  This module is responsible for blocking queries to certain domains.
  """
  use GenServer
  require Logger

  @default_blocklists [
    "https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/hosts/pro.plus.txt"
  ]

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(opts) do
    blocklist = refresh_blocklists()
    {:ok, %{opts: opts, blocklist: blocklist, blocklist_urls: @default_blocklists, whitelisted_domains: []}}
  end

  def filter_query(domain) do
    GenServer.call(__MODULE__, {:filter_query, domain})
  end

  def get_lists() do
    GenServer.call(__MODULE__, :get_lists)
  end

  def handle_call({:filter_query, domain}, _from, state) do
    if blocked?(domain, state.blocklist) do
      {:reply, :blocked, state}
    else
      {:reply, :allowed, state}
    end
  end

  def handle_call(:get_lists, _from, state) do
    {:reply, %{blocklist_urls: state.blocklist_urls, whitelisted_domains: state.whitelisted_domains}, state}
  end

  def handle_call(:stats, _from, state) do
    stats = %{
      total_blocked: length(Map.keys(state.blocklist)),
      total_whitelisted: length(state.whitelisted_domains),
      blocklist_urls: length(state.blocklist_urls)
    }

    {:reply, stats, state}
  end

  defp blocked?(domain, blocklist) do
    domain_parts = String.split(domain, ".")

    Enum.any?(0..(length(domain_parts) - 1), fn i ->
      subdomain = Enum.join(Enum.slice(domain_parts, i, length(domain_parts) - i), ".")
      Map.has_key?(blocklist, subdomain)
    end)
  end

  def stats(), do: GenServer.call(__MODULE__, :stats)

  defp refresh_blocklists do
    @default_blocklists
    |> Enum.map(fn blocklist_url ->
      case HTTPoison.get(blocklist_url) do
        {:ok, %HTTPoison.Response{body: body}} ->
          body
          |> String.split("\n")
          |> Enum.filter(fn line -> not String.starts_with?(line, "#") end)
          |> Enum.map(fn line -> String.split(line) |> List.last() end)
          |> Enum.reduce(%{}, fn domain, acc -> Map.put(acc, domain, true) end)

        {:error, %HTTPoison.Error{reason: reason}} ->
          Logger.error("Failed to load blocklist: #{reason}")
          %{}
      end
    end)
    |> Enum.reduce(%{}, fn blocklist, acc -> Map.merge(acc, blocklist) end)
  end
end
