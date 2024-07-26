defmodule HeimdallWeb.Blocklists.IndexLive do
  alias Heimdall.Servers.Blocker
  use HeimdallWeb, :live_view

  def mount(_params, _session, socket) do
    %{blocklist_urls: blocklist_urls, whitelisted_domains: whitelisted_domains} = Blocker.get_lists()
    socket = socket |> assign(:blocklist_urls, blocklist_urls) |> assign(:whitelisted_domains, whitelisted_domains) |> assign(:blocked_domains, [])
    {:ok, socket}
  end
end
