defmodule HeimdallWeb.Analytics.IndexLive do
  alias Heimdall.Servers.Blocker
  alias Heimdall.Servers.Tracker
  alias Heimdall.DNS.Cache
  use HeimdallWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    if connected?(socket) do
      send(self(), :load_data)
      Phoenix.PubSub.subscribe(Heimdall.PubSub, "queries")
    end

    {:ok,
     socket
     |> assign(:total_queries, 0)
     |> assign(:failed_queries, 0)
     |> assign(:blocked_queries, 0)
     |> assign(:blocked_domains, 0)
     |> assign(:rate_limit_blocked_clients, 0)
     |> assign(:cache_stats, %{})
     |> assign(:recent_queries, [])}
  end

  def handle_info(%{timestamp: _timestamp, domain: _domain, status: _status} = query, socket) do
    socket =
      socket
      |> assign(:recent_queries, Enum.take([query | socket.assigns.recent_queries], 10))

    {:noreply, socket}
  end

  @impl true
  def handle_info(:load_data, socket) do
    tracker_stats = Tracker.get_stats()
    successful_queries = tracker_stats.successful_queries
    failed_queries = tracker_stats.failed_queries
    blocked_queries = tracker_stats.blocked_queries
    blocked_clients = tracker_stats.blocked_clients

    %{
      total_blocked: total_blocked,
      total_whitelisted: _total_whitelisted,
      blocklist_urls: _blocklist_urls
    } = Blocker.stats()

    cache_stats =
      case Cache.stats() do
        {:ok, stats} ->
          %{
            "hits" => stats.hits,
            "misses" => stats.misses,
            "size" => stats.size
          }

        _ ->
          %{}
      end

    socket =
      socket
      |> assign(:total_queries, successful_queries + failed_queries + blocked_queries)
      |> assign(:failed_queries, failed_queries)
      |> assign(:blocked_queries, blocked_queries)
      |> assign(:cache_stats, Enum.to_list(cache_stats))
      |> assign(:blocked_domains, total_blocked)
      |> assign(:rate_limit_blocked_clients, length(Map.keys(blocked_clients)))

    Process.send_after(self(), :load_data, 5000)

    {:noreply, socket}
  end
end
