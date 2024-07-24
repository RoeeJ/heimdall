defmodule Heimdall.Servers.Blocker do
  @moduledoc """
  This module is responsible for blocking queries to certain domains.
  """
  use GenServer
  require Logger

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(opts) do
    blocklist = load_blocklist()
    {:ok, %{opts: opts, blocklist: blocklist}}
  end

  def filter_query(domain) do
    GenServer.call(__MODULE__, {:filter_query, domain})
  end

  def handle_call({:filter_query, domain}, _from, state) do
    if blocked?(domain, state.blocklist) do
      {:reply, :blocked, state}
    else
      {:reply, :allowed, state}
    end
  end

  defp blocked?(domain, blocklist) do
    domain_parts = String.split(domain, ".")
    Enum.any?(0..(length(domain_parts) - 1), fn i ->
      subdomain = Enum.join(Enum.slice(domain_parts, i, length(domain_parts) - i), ".")
      Map.has_key?(blocklist, subdomain)
    end)
  end

  defp load_blocklist do
    blocklist_url = "https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/hosts/pro.plus.txt"

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
  end
end
